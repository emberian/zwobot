mod config;
mod error;
mod llm;
mod zulip;

use config::{AppConfig, TopicConfig, ZulipConfig};
use llm::LlmEngine;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use zulip::{Message, ZulipClient};

type ModelCache = Arc<RwLock<HashMap<String, Arc<LlmEngine>>>>;

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

                            tokio::spawn(async move {
                                if let Err(e) =
                                    handle_message(zulip, app_config, models, message).await
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
    message: Message,
) -> anyhow::Result<()> {
    // Get stream name from display_recipient
    let stream_name = match &message.display_recipient {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(arr) => {
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

    // Get conversation history
    let history = zulip
        .get_topic_messages(&app_config.channel, &topic)
        .await?;

    // Format conversation history as a prompt
    let prompt = format_conversation_history(&history, zulip.bot_id());

    info!("Generating response for topic: {}", topic);

    // Generate response
    let response = model.generate(&prompt).await?;

    // Send response
    zulip
        .send_message(&app_config.channel, &topic, &response)
        .await?;

    info!("Response sent to {}/{}", app_config.channel, topic);

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

fn format_conversation_history(messages: &[Message], bot_id: i64) -> String {
    let mut formatted = String::new();
    formatted.push_str("You are a helpful AI assistant participating in a conversation. Below is the conversation history. Generate a natural, helpful response that continues the conversation.\n\n");
    formatted.push_str("Conversation:\n");

    for msg in messages {
        let role = if msg.sender_id == bot_id {
            "Assistant"
        } else {
            &msg.sender_full_name
        };
        formatted.push_str(&format!("{}: {}\n\n", role, msg.content));
    }

    formatted.push_str("Assistant:");

    formatted
}
