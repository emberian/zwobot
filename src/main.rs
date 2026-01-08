mod bot_control;
mod config;
mod error;
mod llm;
mod prompts;
mod tools;
mod turn;
mod world;
mod zulip;
mod zulip_logger;

use bot_control::{ControlContext, ModelState};
use config::{AppConfig, TopicConfig, ZulipConfig};
use llm::LlmEngine;
use prompts::{build_interpreter_prompt, extract_action, extract_narrative};
use smol_str::SmolStr;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tools::{execute_tool, get_available_tools};
use tracing::{debug, error, info, trace, Level};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use world::WorldState;
use zulip::{Message, ZulipClient};
use zulip_logger::ZulipLayer;

/// Result of attempting to get/load a model
enum ModelLoadResult {
    /// Model is ready
    Ready(Arc<LlmEngine>),
    /// Model download was just started in background
    StartedLoading,
    /// Model was already loading from previous request
    AlreadyLoading,
    /// Model loading failed
    Failed(String),
}

type ModelCache = Arc<RwLock<HashMap<String, ModelState>>>;
type WorldCache = Arc<RwLock<HashMap<String, WorldState>>>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load configuration first (before logging, so we can log to Zulip)
    let app_config = AppConfig::load()?;
    let zulip_config = ZulipConfig::load()?;

    // Connect to Zulip
    let zulip = Arc::new(ZulipClient::new(zulip_config).await?);

    // Initialize logging with both stdout and Zulip
    let env_filter = tracing_subscriber::EnvFilter::from_default_env()
        .add_directive("zwobot=info".parse()?);

    let fmt_layer = tracing_subscriber::fmt::layer();

    // Create Zulip logging layer (logs TRACE and above to bot-logs topic)
    let (zulip_layer, _log_task) = ZulipLayer::new(
        zulip.clone(),
        app_config.channel.clone(),
        "bot-logs".to_string(),
        Level::TRACE,
    );

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .with(zulip_layer)
        .init();

    info!("Starting zwobot...");
    info!("Configuration loaded");
    info!("Monitoring channel: {}", app_config.channel);

    // Model cache: topic -> LlmEngine
    let models: ModelCache = Arc::new(RwLock::new(HashMap::new()));

    // World state cache: topic -> WorldState
    let worlds: WorldCache = Arc::new(RwLock::new(HashMap::new()));

    // Register for real-time message events
    let (mut queue_id, mut last_event_id) = zulip.register_queue(&["message"]).await?;
    info!("Listening for messages...");

    // Event loop
    loop {
        match zulip.get_events(&queue_id, last_event_id).await {
            Ok(events) => {
                for event in events {
                    last_event_id = event.id;

                    if event.event_type == "message" {
                        if let Some(message) = event.message {
                            // Process message in background to avoid blocking event loop
                            let zulip = zulip.clone();
                            let app_config = app_config.clone();
                            let models = models.clone();
                            let worlds = worlds.clone();

                            tokio::spawn(async move {
                                if let Err(e) =
                                    handle_message(zulip, app_config, models, worlds, message).await
                                {
                                    error!("Error handling message: {}", e);
                                }
                            });
                        }
                    }
                }
            }
            Err(e) => {
                error!("Error getting events: {}", e);
                // Wait a bit before retrying
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

                // Try to re-register the queue
                match zulip.register_queue(&["message"]).await {
                    Ok((new_queue_id, new_last_event_id)) => {
                        info!("Re-registered event queue");
                        queue_id = new_queue_id;
                        last_event_id = new_last_event_id;
                    }
                    Err(e) => {
                        error!("Failed to re-register queue: {}", e);
                    }
                }
            }
        }

        // Small delay to avoid hammering the API
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}

async fn handle_message(
    zulip: Arc<ZulipClient>,
    app_config: AppConfig,
    models: ModelCache,
    worlds: WorldCache,
    message: Message,
) -> anyhow::Result<()> {
    // Get stream name from display_recipient
    let stream_name = match &message.display_recipient {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(_arr) => {
            // For stream messages, display_recipient is the stream name as a string
            // For private messages, it's an array of users
            // We only care about stream messages
            return Ok(());
        }
        _ => return Ok(()),
    };

    // Only process messages from our configured channel
    if stream_name != app_config.channel {
        return Ok(());
    }

    // Ignore messages from the bot itself
    if message.sender_id == zulip.bot_id() {
        return Ok(());
    }

    let topic = message.subject.clone();
    let message_id = message.id;
    let player_name = message.sender_full_name.clone();
    let player_message = message.content.clone();

    info!(
        "Received message in {}/{} from {}: {}",
        stream_name,
        topic,
        player_name,
        player_message.chars().take(50).collect::<String>()
    );

    // Add progress indicator (working emoji)
    debug!("Adding progress indicator to message {}", message_id);
    let _ = zulip.add_reaction(message_id, "working").await;

    // Handle special topics
    match topic.as_str() {
        "bot-control" => {
            debug!("Routing to bot-control handler");
            let ctx = ControlContext {
                zulip: zulip.clone(),
                models: models.clone(),
                worlds: worlds.clone(),
                coordinators: Arc::new(RwLock::new(HashMap::new())), // Placeholder
                app_config: app_config.clone(),
            };
            let result = bot_control::handle_control_message(&ctx, &message, "bot-control").await;
            // Remove progress indicator and add completion
            let _ = zulip.remove_reaction(message_id, "working").await;
            let _ = zulip.add_reaction(message_id, "white_check_mark").await;
            return result;
        }
        "bot-logs" => {
            // Ignore messages sent to bot-logs (it's for output only)
            debug!("Ignoring message to bot-logs topic");
            let _ = zulip.remove_reaction(message_id, "working").await;
            return Ok(());
        }
        _ => {
            // Regular game topic, continue processing
        }
    }

    // Get topic config
    let topic_config = app_config.get_topic_config(&topic);
    debug!(
        "Topic config: model={} quant={}",
        topic_config.model_id,
        topic_config.quantization
    );

    // Get or load model for this topic (non-blocking)
    let model = match get_or_load_model(
        models.clone(),
        zulip.clone(),
        &app_config.channel,
        &topic,
        topic_config.clone(),
    )
    .await
    {
        ModelLoadResult::Ready(engine) => engine,
        ModelLoadResult::StartedLoading => {
            // Model download started in background, we've already notified the user
            let _ = zulip.remove_reaction(message_id, "working").await;
            let _ = zulip.add_reaction(message_id, "hourglass").await;
            return Ok(());
        }
        ModelLoadResult::AlreadyLoading => {
            // Model is still loading from a previous request
            let _ = zulip.remove_reaction(message_id, "working").await;
            let _ = zulip.add_reaction(message_id, "hourglass").await;
            return Ok(());
        }
        ModelLoadResult::Failed(err) => {
            let _ = zulip.remove_reaction(message_id, "working").await;
            let _ = zulip.add_reaction(message_id, "x").await;
            zulip
                .send_message(&app_config.channel, &topic, &format!("Model loading failed: {}", err))
                .await?;
            return Ok(());
        }
    };

    // Get or load world state for this topic
    let mut world = get_or_load_world(&worlds, &topic, &app_config.world_data_path).await?;

    // Ensure player exists in world (use Zulip username as character name)
    let player_smol: SmolStr = player_name.clone().into();
    ensure_player_in_world(&mut world, &player_smol);

    info!("Processing player command from {} in topic {}", player_name, topic);

    // === NEW FLOW: Human is player, LLM interprets and narrates ===

    // 1. Build interpreter prompt
    debug!("Building interpreter prompt");
    let prompt = build_interpreter_prompt(&world, &player_name, &player_message);
    trace!("Prompt length: {} chars", prompt.len());

    // 2. LLM interprets player intent and narrates
    debug!("Calling LLM for interpretation");
    let llm_output = model.generate(&prompt).await?;
    info!("LLM output (first 200 chars): {}", llm_output.chars().take(200).collect::<String>());

    // 3. Extract action and narrative from LLM output
    let action_str = extract_action(&llm_output);
    let narrative = extract_narrative(&llm_output);

    // 4. Parse and execute the action
    let response = if let Some(action) = action_str {
        debug!("Extracted action: {}", action);

        // Parse into tool call format
        let parts: Vec<&str> = action.split_whitespace().collect();
        if parts.is_empty() {
            format!("{}\n\n*[No action understood]*", narrative)
        } else {
            let tool_name = parts[0].to_lowercase();
            let args: Vec<SmolStr> = parts[1..].iter().map(|s| (*s).into()).collect();

            let tool_call = tools::definitions::ToolCall {
                thinking: String::new(),
                tool_name: tool_name.into(),
                args,
            };

            // Execute the tool
            match execute_tool(&mut world, &player_smol, &tool_call) {
                Ok(result) => {
                    debug!("Tool execution successful: {}", result.summary);

                    // Build response with narrative + affordances
                    let affordances = format_affordances_for_player(&world, &player_smol);
                    format!("{}\n\n---\n*{}*", narrative, affordances)
                }
                Err(e) => {
                    error!("Tool execution error: {}", e);
                    format!("{}\n\n*[Error: {}]*", narrative, e)
                }
            }
        }
    } else {
        // No action found - just return the narrative
        debug!("No action tag found in LLM output");
        narrative
    };

    // 5. Send response to Zulip
    debug!("Sending response to Zulip");
    zulip
        .send_message(&app_config.channel, &topic, &response)
        .await?;

    // 6. Save world state
    debug!("Saving world state");
    save_world(&worlds, &topic, world).await?;

    // Remove progress indicator and add completion emoji
    debug!("Removing progress indicator from message {}", message_id);
    let _ = zulip.remove_reaction(message_id, "working").await;
    let _ = zulip.add_reaction(message_id, "white_check_mark").await;

    debug!("Message handling complete for topic: {}", topic);
    Ok(())
}

/// Format available actions as a player-friendly hint
fn format_affordances_for_player(world: &WorldState, player: &SmolStr) -> String {
    let char_state = match world.get_character(player) {
        Some(cs) => cs,
        None => return "You can: look around".to_string(),
    };

    let room = match world.spatial.rooms.get(&char_state.location) {
        Some(r) => r,
        None => return "You can: look around".to_string(),
    };

    let mut hints = Vec::new();

    // Movement
    if !room.exits.is_empty() {
        let exits: Vec<String> = room.exits.keys().map(|s| s.to_string()).collect();
        hints.push(format!("go {}", exits.join("/")));
    }

    // Items
    if !room.objects.is_empty() {
        hints.push("examine/take items".to_string());
    }

    // NPCs
    if !room.npcs.is_empty() {
        let npc_names: Vec<String> = room
            .npcs
            .iter()
            .filter_map(|id| world.npc_defs.get(id))
            .map(|npc| npc.name.to_string())
            .collect();
        hints.push(format!("talk to {}", npc_names.join(", ")));
    }

    // Inventory
    if !char_state.inventory.is_empty() {
        hints.push("check inventory".to_string());
    }

    hints.push("look around".to_string());

    format!("You can: {}", hints.join(" | "))
}

async fn get_or_load_model(
    models: ModelCache,
    zulip: Arc<ZulipClient>,
    channel: &str,
    topic: &str,
    config: TopicConfig,
) -> ModelLoadResult {
    // Check current state
    {
        let models_read = models.read().await;
        if let Some(state) = models_read.get(topic) {
            match state {
                ModelState::Ready(engine) => {
                    debug!("Using cached model for topic '{}'", topic);
                    return ModelLoadResult::Ready(engine.clone());
                }
                ModelState::Loading => {
                    debug!("Model for topic '{}' is still loading", topic);
                    return ModelLoadResult::AlreadyLoading;
                }
                ModelState::Failed(err) => {
                    debug!("Model for topic '{}' previously failed: {}", topic, err);
                    return ModelLoadResult::Failed(err.clone());
                }
            }
        }
    }

    // Mark as loading
    {
        let mut models_write = models.write().await;
        models_write.insert(topic.to_string(), ModelState::Loading);
    }

    // Notify user that we're downloading
    info!(
        "Starting model download for topic '{}': {} {}",
        topic, config.model_id, config.quantization
    );
    let _ = zulip
        .send_message(
            channel,
            topic,
            &format!(
                "Downloading model **{}** ({})... I'll respond once it's ready.",
                config.model_id, config.quantization
            ),
        )
        .await;

    // Spawn the download in background
    let models_clone = models.clone();
    let topic_owned = topic.to_string();
    tokio::spawn(async move {
        debug!(
            "Background model loading started for topic '{}'",
            topic_owned
        );

        let result = LlmEngine::new(
            &config.model_id,
            &config.quantization,
            config.max_tokens,
            config.temperature,
        )
        .await;

        let mut models_write = models_clone.write().await;
        match result {
            Ok(engine) => {
                info!("Model loaded successfully for topic '{}'", topic_owned);
                models_write.insert(topic_owned, ModelState::Ready(Arc::new(engine)));
            }
            Err(e) => {
                error!("Model loading failed for topic '{}': {}", topic_owned, e);
                models_write.insert(topic_owned, ModelState::Failed(e.to_string()));
            }
        }
    });

    ModelLoadResult::StartedLoading
}

async fn get_or_load_world(
    worlds: &WorldCache,
    topic: &str,
    template_path: &str,
) -> anyhow::Result<WorldState> {
    // Check if world is already loaded for this topic
    {
        let worlds_read = worlds.read().await;
        if let Some(world) = worlds_read.get(topic) {
            debug!("Using cached world for topic '{}'", topic);
            trace!("Cached world has {} characters", world.characters.len());
            return Ok(world.clone());
        }
    }

    // World not loaded - try to load from save file, fall back to template
    let save_path = WorldState::save_path_for_topic(topic);
    debug!("Loading world for topic '{}' from: {}", topic, save_path);
    let world = WorldState::load_or_create(&save_path, template_path)?;
    debug!("World loaded with {} rooms, {} objects, {} NPCs",
        world.spatial.rooms.len(),
        world.object_defs.len(),
        world.npc_defs.len()
    );

    // Store in cache
    {
        let mut worlds_write = worlds.write().await;
        worlds_write.insert(topic.to_string(), world.clone());
        debug!("World cached for topic '{}'", topic);
    }

    info!("World state loaded for topic '{}'", topic);

    Ok(world)
}

async fn save_world(
    worlds: &WorldCache,
    topic: &str,
    world: WorldState,
) -> anyhow::Result<()> {
    // Save to disk
    let save_path = WorldState::save_path_for_topic(topic);
    world.save_to_file(&save_path)?;
    info!("World state saved to {}", save_path);

    // Update cache
    {
        let mut worlds_write = worlds.write().await;
        worlds_write.insert(topic.to_string(), world);
    }

    Ok(())
}

fn ensure_player_in_world(world: &mut WorldState, player_name: &SmolStr) {
    // Check if player already exists
    if world.get_character(player_name).is_some() {
        trace!("Player {} already exists in world", player_name);
        return;
    }

    // Add player to starting location (tavern)
    let starting_location: SmolStr = "tavern".into();

    info!("Adding new player {} to world at {}", player_name, starting_location);
    debug!("Player created with default stats");

    world.add_character(player_name.clone(), starting_location);
}
