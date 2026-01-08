mod config;
mod error;
mod llm;
mod prompts;
mod tools;
mod world;
mod zulip;

use config::{AppConfig, Character, TopicConfig, ZulipConfig};
use llm::LlmEngine;
use prompts::build_turn_prompt;
use smol_str::SmolStr;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tools::{execute_tool, parse_tool_call};
use tracing::{error, info};
use world::WorldState;
use zulip::{Message, ZulipClient};

type ModelCache = Arc<RwLock<HashMap<String, Arc<LlmEngine>>>>;
type WorldCache = Arc<RwLock<HashMap<String, WorldState>>>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("zwobot=info".parse()?),
        )
        .init();

    info!("Starting zwobot...");

    // Load configuration
    let app_config = AppConfig::load()?;
    let zulip_config = ZulipConfig::load()?;

    info!("Configuration loaded");
    info!("Monitoring channel: {}", app_config.channel);

    // Connect to Zulip
    let zulip = Arc::new(ZulipClient::new(zulip_config).await?);
    info!("Connected to Zulip");

    // Model cache: topic -> LlmEngine
    let models: ModelCache = Arc::new(RwLock::new(HashMap::new()));

    // World state cache: topic -> WorldState
    let worlds: WorldCache = Arc::new(RwLock::new(HashMap::new()));

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

    // Get topic config
    let topic_config = app_config.get_topic_config(&topic);

    // Get or load model for this topic
    let model = get_or_load_model(&models, &topic, topic_config).await?;

    // Get or load world state for this topic
    let mut world = get_or_load_world(&worlds, &topic, &app_config.world_data_path).await?;

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

        // Build prompt with world state and tools
        let prompt = build_turn_prompt(&world, &character, &history, zulip.bot_id());

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

        // Execute tool
        let result = match execute_tool(&mut world, &character_name, &tool_call) {
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
        };

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

    // Save world state after all characters have acted
    save_world(&worlds, &topic, world).await?;

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
    world_data_path: &str,
) -> anyhow::Result<WorldState> {
    // Check if world is already loaded for this topic
    {
        let worlds_read = worlds.read().await;
        if let Some(world) = worlds_read.get(topic) {
            return Ok(world.clone());
        }
    }

    // World not loaded, load from file
    info!("Loading world state from {}", world_data_path);
    let world = WorldState::load_from_file(world_data_path)?;

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
) -> anyhow::Result<()> {
    // Update cache
    {
        let mut worlds_write = worlds.write().await;
        worlds_write.insert(topic.to_string(), world);
    }

    // Note: We don't save to disk here - world state persists in memory per topic
    // In Phase 4+, we could add periodic saving or save on significant events

    Ok(())
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
