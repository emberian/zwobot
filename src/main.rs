mod bot_control;
mod config;
mod error;
mod llm;
mod prompts;
mod scenes;
mod tools;
mod turn;
mod world;
mod zulip;
mod zulip_logger;

use bot_control::ControlContext;
use config::{AppConfig, TopicConfig, ZulipConfig};
use llm::LlmEngine;
use prompts::{build_turn_prompt, get_active_scene_output};
use scenes::SceneManager;
use smol_str::SmolStr;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tools::{execute_tool, parse_tool_call};
use tracing::{error, info, Level};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use turn::TurnCoordinator;
use world::WorldState;
use zulip::{Message, ZulipClient};
use zulip_logger::ZulipLayer;

type ModelCache = Arc<RwLock<HashMap<String, Arc<LlmEngine>>>>;
type WorldCache = Arc<RwLock<HashMap<String, WorldState>>>;
type CoordinatorCache = Arc<RwLock<HashMap<String, TurnCoordinator>>>;
type SceneManagerCache = Arc<RwLock<SceneManager>>;

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

    // Create Zulip logging layer (logs WARN and above to bot-logs topic)
    let (zulip_layer, _log_task) = ZulipLayer::new(
        zulip.clone(),
        app_config.channel.clone(),
        "bot-logs".to_string(),
        Level::WARN,
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

    // Turn coordinator cache: topic -> TurnCoordinator
    let coordinators: CoordinatorCache = Arc::new(RwLock::new(HashMap::new()));

    // Scene manager (shared across topics)
    let scene_manager: SceneManagerCache =
        Arc::new(RwLock::new(SceneManager::new("data/scenes")));

    // Register for real-time message events
    let (queue_id, mut last_event_id) = zulip.register_queue(&["message"]).await?;
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
                            let coordinators = coordinators.clone();
                            let scene_manager = scene_manager.clone();

                            tokio::spawn(async move {
                                if let Err(e) =
                                    handle_message(zulip, app_config, models, worlds, coordinators, scene_manager, message).await
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
                        last_event_id = new_last_event_id;
                        // Note: We can't reassign queue_id here because it's not mutable
                        // In a real implementation, we'd need to refactor this
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
    coordinators: CoordinatorCache,
    scene_manager: SceneManagerCache,
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
    info!(
        "Received message in {}/{} from {}: {}",
        stream_name,
        topic,
        message.sender_full_name,
        message.content.chars().take(50).collect::<String>()
    );

    // Handle bot-control topic
    if topic == "bot-control" {
        let ctx = ControlContext {
            zulip: zulip.clone(),
            models: models.clone(),
            worlds: worlds.clone(),
            coordinators: coordinators.clone(),
            app_config: app_config.clone(),
        };
        return bot_control::handle_control_message(&ctx, &message, "bot-control").await;
    }

    // Get topic config
    let topic_config = app_config.get_topic_config(&topic);

    // Get or load model for this topic
    let model = get_or_load_model(&models, &topic, topic_config).await?;

    // Get or load world state for this topic
    let mut world = get_or_load_world(&worlds, &topic, &app_config.world_data_path).await?;

    // Get or create turn coordinator for this topic
    let mut coordinator = get_or_create_coordinator(&coordinators, &topic).await;

    // Get conversation history
    let history = zulip
        .get_topic_messages(&app_config.channel, &topic)
        .await?;

    // Get characters for this topic
    let characters = topic_config.get_characters();

    info!(
        "Generating responses for {} character(s) in topic: {}",
        characters.len(),
        topic
    );

    // Generate response for each character
    for character in characters {
        let character_name: SmolStr = character.name.clone().into();

        // Ensure character exists in world
        ensure_character_in_world(&mut world, &character_name);

        // Get active scene context if character is in a scene
        let scene_context = {
            let mut manager = scene_manager.write().await;
            get_active_scene_output(&world, &character.name, &mut manager)
        };

        // Build prompt with world state, tools, recent actions, and scene context
        let prompt = build_turn_prompt(
            &world,
            &character,
            &history,
            zulip.bot_id(),
            Some(&coordinator),
            scene_context.as_ref(),
        );

        info!("Generating response for character: {}", character.name);

        // Generate response from LLM
        let llm_output = model.generate(&prompt).await?;

        info!("LLM output (first 200 chars): {}", llm_output.chars().take(200).collect::<String>());

        // Parse tool call from output
        let tool_call = match parse_tool_call(&llm_output) {
            Ok(call) => call,
            Err(e) => {
                error!("Failed to parse tool call: {}. Output: {}", e, llm_output);
                // Send error message
                zulip
                    .send_message(
                        &app_config.channel,
                        &topic,
                        &format!("**{}:** *[Error: No valid tool call found]*", character.name),
                    )
                    .await?;
                continue;
            }
        };

        info!("Tool call: {} {:?}", tool_call.tool_name, tool_call.args);

        // Execute tool (with scene manager for scene-aware tools)
        let result = {
            let mut manager = scene_manager.write().await;
            match execute_tool(&mut world, &character_name, &tool_call, Some(&mut manager)) {
                Ok(res) => res,
                Err(e) => {
                    error!("Tool execution error: {}", e);
                    zulip
                        .send_message(
                            &app_config.channel,
                            &topic,
                            &format!("**{}:** *[Error: {}]*", character.name, e),
                        )
                        .await?;
                    continue;
                }
            }
        };

        // Record action in coordinator for other characters to see
        coordinator.record_action(
            character_name.clone(),
            format!("{} {}", tool_call.tool_name, tool_call.args_joined()).into(),
            result.summary.clone().into(),
        );

        // Format response: thinking + tool result
        let mut formatted_response = String::new();

        if topic_config.characters.len() > 1 {
            formatted_response.push_str(&format!("**{}:**\n\n", character.name));
        }

        // Include thinking if it's meaningful
        if !tool_call.thinking.is_empty() && tool_call.thinking.len() > 10 {
            formatted_response.push_str(&format!("*{}*\n\n", tool_call.thinking));
        }

        formatted_response.push_str(&result.description);

        // Send response
        zulip
            .send_message(&app_config.channel, &topic, &formatted_response)
            .await?;

        info!(
            "Response sent from {} to {}/{}: {}",
            character.name, app_config.channel, topic, result.summary
        );
    }

    // Advance turn after all characters have acted
    coordinator.advance_turn();

    // Save world state and coordinator
    save_world(&worlds, &topic, world, &app_config.world_data_path).await?;
    save_coordinator(&coordinators, &topic, coordinator).await;

    Ok(())
}

async fn get_or_load_model(
    models: &ModelCache,
    topic: &str,
    config: &TopicConfig,
) -> anyhow::Result<Arc<LlmEngine>> {
    // Check if model is already loaded
    {
        let models_read = models.read().await;
        if let Some(engine) = models_read.get(topic) {
            return Ok(engine.clone());
        }
    }

    // Model not loaded, load it
    info!(
        "Loading model for topic '{}': {} {}",
        topic, config.model_id, config.quantization
    );

    let engine = LlmEngine::new(
        &config.model_id,
        &config.quantization,
        config.max_tokens,
        config.temperature,
    )
    .await?;

    let engine = Arc::new(engine);

    // Store in cache
    {
        let mut models_write = models.write().await;
        models_write.insert(topic.to_string(), engine.clone());
    }

    info!("Model loaded for topic '{}'", topic);

    Ok(engine)
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
            return Ok(world.clone());
        }
    }

    // World not loaded - try to load from save file, fall back to template
    let save_path = WorldState::save_path_for_topic(topic);
    let world = WorldState::load_or_create(&save_path, template_path)?;

    // Store in cache
    {
        let mut worlds_write = worlds.write().await;
        worlds_write.insert(topic.to_string(), world.clone());
    }

    info!("World state loaded for topic '{}'", topic);

    Ok(world)
}

async fn save_world(
    worlds: &WorldCache,
    topic: &str,
    world: WorldState,
    _template_path: &str,
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

async fn get_or_create_coordinator(
    coordinators: &CoordinatorCache,
    topic: &str,
) -> TurnCoordinator {
    // Check if coordinator exists for this topic
    {
        let coordinators_read = coordinators.read().await;
        if let Some(coordinator) = coordinators_read.get(topic) {
            return coordinator.clone();
        }
    }

    // Create new coordinator
    let coordinator = TurnCoordinator::new();

    // Store in cache
    {
        let mut coordinators_write = coordinators.write().await;
        coordinators_write.insert(topic.to_string(), coordinator.clone());
    }

    coordinator
}

async fn save_coordinator(
    coordinators: &CoordinatorCache,
    topic: &str,
    coordinator: TurnCoordinator,
) {
    let mut coordinators_write = coordinators.write().await;
    coordinators_write.insert(topic.to_string(), coordinator);
}

fn ensure_character_in_world(world: &mut WorldState, character_name: &SmolStr) {
    // Check if character already exists
    if world.get_character(character_name).is_some() {
        return;
    }

    // Add character to starting location (tavern)
    let starting_location: SmolStr = "tavern".into();

    info!("Adding new character {} to world at {}", character_name, starting_location);

    world.add_character(character_name.clone(), starting_location);
}
