//! Bot control interface - admin commands via a special Zulip topic.
//!
//! Commands available in the bot-control topic:
//! - `status` - Show bot status
//! - `topics` - List active topics and their models
//! - `reload <topic>` - Reload world state for a topic
//! - `clear <topic>` - Clear cached model for a topic
//! - `eval <rust_expr>` - Evaluate a Rust expression (future)

use crate::config::AppConfig;
use crate::zulip::{Message, ZulipClient};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

type ModelCache = Arc<RwLock<HashMap<String, Arc<crate::llm::LlmEngine>>>>;
type WorldCache = Arc<RwLock<HashMap<String, crate::world::WorldState>>>;
type CoordinatorCache = Arc<RwLock<HashMap<String, crate::turn::TurnCoordinator>>>;

pub struct ControlContext {
    pub zulip: Arc<ZulipClient>,
    pub models: ModelCache,
    pub worlds: WorldCache,
    pub coordinators: CoordinatorCache,
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
        cmd if cmd.starts_with("reload ") => {
            let topic = cmd.strip_prefix("reload ").unwrap().trim();
            handle_reload(ctx, topic).await
        }
        cmd if cmd.starts_with("clear ") => {
            let topic = cmd.strip_prefix("clear ").unwrap().trim();
            handle_clear(ctx, topic).await
        }
        "help" => {
            Ok("**Bot Control Commands:**\n\n\
                - `status` - Show bot status\n\
                - `topics` - List active topics and their models\n\
                - `reload <topic>` - Reload world state from disk\n\
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
    let worlds_count = ctx.worlds.read().await.len();
    let coordinators_count = ctx.coordinators.read().await.len();

    Ok(format!(
        "**Bot Status**\n\n\
         - Loaded models: {}\n\
         - Active worlds: {}\n\
         - Turn coordinators: {}\n\
         - Monitoring channel: {}\n",
        models_count, worlds_count, coordinators_count, ctx.app_config.channel
    ))
}

async fn handle_topics(ctx: &ControlContext) -> Result<String> {
    let models = ctx.models.read().await;
    let worlds = ctx.worlds.read().await;

    if models.is_empty() && worlds.is_empty() {
        return Ok("No active topics.".to_string());
    }

    let mut response = String::from("**Active Topics:**\n\n");

    // Collect all topic names from both caches
    let mut topics: Vec<&String> = models.keys().chain(worlds.keys()).collect();
    topics.sort();
    topics.dedup();

    for topic in topics {
        let topic_config = ctx.app_config.get_topic_config(topic);
        let has_model = models.contains_key(topic);
        let has_world = worlds.contains_key(topic);

        response.push_str(&format!(
            "**{}**\n  Model: {} ({})\n  Characters: {}\n  Cached: {} {}\n\n",
            topic,
            topic_config.model_id,
            topic_config.quantization,
            topic_config
                .characters
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            if has_model { "✓ model" } else { "" },
            if has_world { "✓ world" } else { "" },
        ));
    }

    Ok(response)
}

async fn handle_reload(ctx: &ControlContext, topic: &str) -> Result<String> {
    // Remove from world cache to force reload
    {
        let mut worlds = ctx.worlds.write().await;
        worlds.remove(topic);
    }

    info!("Cleared world cache for topic: {}", topic);

    Ok(format!(
        "✓ Cleared world cache for topic `{}`\n\nWorld will reload on next message.",
        topic
    ))
}

async fn handle_clear(ctx: &ControlContext, topic: &str) -> Result<String> {
    // Remove from model cache
    let removed = {
        let mut models = ctx.models.write().await;
        models.remove(topic).is_some()
    };

    if removed {
        info!("Cleared model cache for topic: {}", topic);
        Ok(format!(
            "✓ Cleared model cache for topic `{}`\n\nModel will reload on next message.",
            topic
        ))
    } else {
        Ok(format!("No cached model found for topic `{}`", topic))
    }
}
