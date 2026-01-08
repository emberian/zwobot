//! Bot control interface - admin commands via a special Zulip topic.
//!
//! Commands available in the bot-control topic:
//! - `status` - Show bot status
//! - `topics` - List active topics and their models
//! - `clear <topic>` - Clear cached model for a topic

use crate::config::AppConfig;
use crate::llm::LlmEngine;
use crate::zulip::{Message, ZulipClient};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Model loading state - tracks whether a model is loading, loaded, or failed
#[derive(Clone)]
pub enum ModelState {
    /// Model is currently being downloaded/loaded
    Loading,
    /// Model is ready to use
    Ready(Arc<LlmEngine>),
    /// Model failed to load
    Failed(String),
}

pub type ModelCache = Arc<RwLock<HashMap<String, ModelState>>>;

pub struct ControlContext {
    pub zulip: Arc<ZulipClient>,
    pub models: ModelCache,
    pub app_config: AppConfig,
}

/// Handle a message in the bot-control topic
pub async fn handle_control_message(
    ctx: &ControlContext,
    message: &Message,
    control_topic: &str,
) -> Result<()> {
    let content = message.content.trim();

    info!("Control command: {}", content);

    let response = match content {
        "status" => handle_status(ctx).await,
        "topics" => handle_topics(ctx).await,
        cmd if cmd.starts_with("clear ") => {
            let topic = cmd.strip_prefix("clear ").unwrap().trim();
            handle_clear(ctx, topic).await
        }
        "help" => {
            Ok("**Bot Control Commands:**\n\n\
                - `status` - Show bot status\n\
                - `topics` - List active topics and their models\n\
                - `clear <topic>` - Clear cached model\n\
                - `help` - Show this help\n"
                .to_string())
        }
        _ => Ok(format!("Unknown command: `{}`\n\nTry `help` for available commands.", content)),
    };

    match response {
        Ok(msg) => {
            ctx.zulip
                .send_message(&ctx.app_config.channel, control_topic, &msg)
                .await?;
        }
        Err(e) => {
            warn!("Control command error: {}", e);
            ctx.zulip
                .send_message(
                    &ctx.app_config.channel,
                    control_topic,
                    &format!("**Error:** {}", e),
                )
                .await?;
        }
    }

    Ok(())
}

async fn handle_status(ctx: &ControlContext) -> Result<String> {
    let models_count = ctx.models.read().await.len();

    Ok(format!(
        "**Bot Status**\n\n\
         - Loaded models: {}\n\
         - Monitoring channel: {}\n",
        models_count, ctx.app_config.channel
    ))
}

async fn handle_topics(ctx: &ControlContext) -> Result<String> {
    let models = ctx.models.read().await;

    if models.is_empty() {
        return Ok("No active topics.".to_string());
    }

    let mut response = String::from("**Active Topics:**\n\n");

    let mut topics: Vec<&String> = models.keys().collect();
    topics.sort();

    for topic in topics {
        let topic_config = ctx.app_config.get_topic_config(topic);
        let model_status = models.get(topic).map(|s| match s {
            ModelState::Loading => "loading",
            ModelState::Ready(_) => "ready",
            ModelState::Failed(_) => "failed",
        }).unwrap_or("unknown");

        response.push_str(&format!(
            "**{}**\n  Model: {} ({})\n  Status: {}\n\n",
            topic,
            topic_config.model_id,
            topic_config.quantization,
            model_status,
        ));
    }

    Ok(response)
}

async fn handle_clear(ctx: &ControlContext, topic: &str) -> Result<String> {
    let removed = {
        let mut models = ctx.models.write().await;
        models.remove(topic).is_some()
    };

    if removed {
        info!("Cleared model cache for topic: {}", topic);
        Ok(format!(
            "Cleared model cache for topic `{}`\n\nModel will reload on next message.",
            topic
        ))
    } else {
        Ok(format!("No cached model found for topic `{}`", topic))
    }
}
