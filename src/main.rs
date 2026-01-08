mod bot_control;
mod config;
mod error;
mod llm;
mod zulip;
mod zulip_logger;

use bot_control::{ControlContext, ModelCache, ModelState};
use config::{AppConfig, ModelConfig, ZulipConfig};
use llm::LlmEngine;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, trace, Level};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
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

/// Debate state for a topic
#[derive(Clone, Default)]
struct DebateState {
    /// History of the debate: (speaker, message) pairs
    history: Vec<(String, String)>,
    /// Current debate topic/proposition
    proposition: Option<String>,
}

type DebateCache = Arc<RwLock<HashMap<String, DebateState>>>;

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

    info!("Starting zwobot debate club...");
    info!("Configuration loaded");
    info!("Monitoring channel: {}", app_config.channel);

    // Model cache: topic -> LlmEngine (for opponent models)
    let models: ModelCache = Arc::new(RwLock::new(HashMap::new()));

    // Debate state cache: topic -> DebateState
    let debates: DebateCache = Arc::new(RwLock::new(HashMap::new()));

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
                            let debates = debates.clone();

                            tokio::spawn(async move {
                                if let Err(e) =
                                    handle_message(zulip, app_config, models, debates, message).await
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
    debates: DebateCache,
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
    let sender_name = message.sender_full_name.clone();
    let user_message = message.content.clone();

    info!(
        "Received message in {}/{} from {}: {}",
        stream_name,
        topic,
        sender_name,
        user_message.chars().take(50).collect::<String>()
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
                app_config: app_config.clone(),
            };
            let result = bot_control::handle_control_message(&ctx, &message, "bot-control").await;
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
            // Debate club topic, continue processing
        }
    }

    // Check for special commands
    let trimmed = user_message.trim().to_lowercase();

    // "judge" command - get the large LLM to judge the debate
    if trimmed == "judge" || trimmed == "judge!" || trimmed.starts_with("judge the debate") {
        return handle_judge(
            zulip,
            app_config,
            models,
            debates,
            &topic,
            message_id,
        ).await;
    }

    // "new debate:" command - start a new debate with a proposition
    if trimmed.starts_with("new debate:") || trimmed.starts_with("debate:") {
        let proposition = if trimmed.starts_with("new debate:") {
            user_message.trim()[11..].trim().to_string()
        } else {
            user_message.trim()[7..].trim().to_string()
        };

        return handle_new_debate(
            zulip,
            app_config,
            debates,
            &topic,
            &sender_name,
            &proposition,
            message_id,
        ).await;
    }

    // "reset" command - clear the debate history
    if trimmed == "reset" || trimmed == "clear" || trimmed == "new" {
        {
            let mut debates_write = debates.write().await;
            debates_write.remove(&topic);
        }
        let _ = zulip.remove_reaction(message_id, "working").await;
        let _ = zulip.add_reaction(message_id, "white_check_mark").await;
        zulip.send_message(&app_config.channel, &topic, "Debate cleared. Start a new one with `debate: <proposition>`").await?;
        return Ok(());
    }

    // Regular debate message - human is making an argument
    handle_debate_turn(
        zulip,
        app_config,
        models,
        debates,
        &topic,
        &sender_name,
        &user_message,
        message_id,
    ).await
}

async fn handle_new_debate(
    zulip: Arc<ZulipClient>,
    app_config: AppConfig,
    debates: DebateCache,
    topic: &str,
    sender_name: &str,
    proposition: &str,
    message_id: i64,
) -> anyhow::Result<()> {
    // Initialize new debate state
    {
        let mut debates_write = debates.write().await;
        debates_write.insert(topic.to_string(), DebateState {
            history: vec![],
            proposition: Some(proposition.to_string()),
        });
    }

    let response = format!(
        "**New Debate Started!**\n\n\
         **Proposition:** {}\n\n\
         {} will argue **FOR** the proposition.\n\
         I will argue **AGAINST** it.\n\n\
         Make your opening argument! When you're ready for judgment, say `judge`.",
        proposition, sender_name
    );

    let _ = zulip.remove_reaction(message_id, "working").await;
    let _ = zulip.add_reaction(message_id, "white_check_mark").await;
    zulip.send_message(&app_config.channel, topic, &response).await?;

    Ok(())
}

async fn handle_debate_turn(
    zulip: Arc<ZulipClient>,
    app_config: AppConfig,
    models: ModelCache,
    debates: DebateCache,
    topic: &str,
    sender_name: &str,
    user_message: &str,
    message_id: i64,
) -> anyhow::Result<()> {
    // Get or create debate state
    let (debate_state, proposition) = {
        let debates_read = debates.read().await;
        match debates_read.get(topic) {
            Some(state) => (state.clone(), state.proposition.clone()),
            None => {
                // No active debate - prompt user to start one
                let _ = zulip.remove_reaction(message_id, "working").await;
                zulip.send_message(
                    &app_config.channel,
                    topic,
                    "No active debate. Start one with `debate: <your proposition>`"
                ).await?;
                return Ok(());
            }
        }
    };

    let proposition = proposition.unwrap_or_else(|| "the given topic".to_string());

    // Add human's argument to history
    {
        let mut debates_write = debates.write().await;
        if let Some(state) = debates_write.get_mut(topic) {
            state.history.push((sender_name.to_string(), user_message.to_string()));
        }
    }

    // Get or load opponent model
    let model = match get_or_load_model(
        models.clone(),
        zulip.clone(),
        &app_config.channel,
        topic,
        &app_config.debate_opponent,
    ).await {
        ModelLoadResult::Ready(engine) => engine,
        ModelLoadResult::StartedLoading | ModelLoadResult::AlreadyLoading => {
            let _ = zulip.remove_reaction(message_id, "working").await;
            let _ = zulip.add_reaction(message_id, "hourglass").await;
            return Ok(());
        }
        ModelLoadResult::Failed(err) => {
            let _ = zulip.remove_reaction(message_id, "working").await;
            let _ = zulip.add_reaction(message_id, "x").await;
            zulip.send_message(&app_config.channel, topic, &format!("Model loading failed: {}", err)).await?;
            return Ok(());
        }
    };

    // Build prompt for opponent
    let prompt = build_opponent_prompt(&debate_state, &proposition, sender_name, user_message);
    trace!("Opponent prompt: {}", prompt);

    // Generate opponent response
    debug!("Generating opponent response");
    let opponent_response = model.generate(&prompt).await?;
    info!("Opponent response (first 200 chars): {}", opponent_response.chars().take(200).collect::<String>());

    // Add opponent's response to history
    {
        let mut debates_write = debates.write().await;
        if let Some(state) = debates_write.get_mut(topic) {
            state.history.push(("Opponent".to_string(), opponent_response.clone()));
        }
    }

    // Send response
    let _ = zulip.remove_reaction(message_id, "working").await;
    let _ = zulip.add_reaction(message_id, "white_check_mark").await;
    zulip.send_message(&app_config.channel, topic, &opponent_response).await?;

    Ok(())
}

async fn handle_judge(
    zulip: Arc<ZulipClient>,
    app_config: AppConfig,
    models: ModelCache,
    debates: DebateCache,
    topic: &str,
    message_id: i64,
) -> anyhow::Result<()> {
    // Get debate state
    let debate_state = {
        let debates_read = debates.read().await;
        match debates_read.get(topic) {
            Some(state) => state.clone(),
            None => {
                let _ = zulip.remove_reaction(message_id, "working").await;
                zulip.send_message(&app_config.channel, topic, "No debate to judge. Start one with `debate: <proposition>`").await?;
                return Ok(());
            }
        }
    };

    if debate_state.history.is_empty() {
        let _ = zulip.remove_reaction(message_id, "working").await;
        zulip.send_message(&app_config.channel, topic, "The debate has no arguments yet!").await?;
        return Ok(());
    }

    // Load the judge model (this uses a special cache key)
    let judge_cache_key = format!("{}_judge", topic);
    let judge = match get_or_load_model(
        models.clone(),
        zulip.clone(),
        &app_config.channel,
        &judge_cache_key,
        &app_config.debate_judge,
    ).await {
        ModelLoadResult::Ready(engine) => engine,
        ModelLoadResult::StartedLoading | ModelLoadResult::AlreadyLoading => {
            let _ = zulip.remove_reaction(message_id, "working").await;
            let _ = zulip.add_reaction(message_id, "hourglass").await;
            zulip.send_message(&app_config.channel, topic, "Loading the judge model... I'll render judgment soon.").await?;
            return Ok(());
        }
        ModelLoadResult::Failed(err) => {
            let _ = zulip.remove_reaction(message_id, "working").await;
            let _ = zulip.add_reaction(message_id, "x").await;
            zulip.send_message(&app_config.channel, topic, &format!("Judge model loading failed: {}", err)).await?;
            return Ok(());
        }
    };

    // Build judge prompt
    let prompt = build_judge_prompt(&debate_state);
    trace!("Judge prompt: {}", prompt);

    // Generate judgment
    debug!("Generating judgment");
    let judgment = judge.generate(&prompt).await?;
    info!("Judgment (first 200 chars): {}", judgment.chars().take(200).collect::<String>());

    // Clear the debate after judgment
    {
        let mut debates_write = debates.write().await;
        debates_write.remove(topic);
    }

    // Send judgment
    let response = format!("**The Judge's Verdict**\n\n{}\n\n---\n*Debate concluded. Start a new one with `debate: <proposition>`*", judgment);
    let _ = zulip.remove_reaction(message_id, "working").await;
    let _ = zulip.add_reaction(message_id, "scales").await;
    zulip.send_message(&app_config.channel, topic, &response).await?;

    Ok(())
}

fn build_opponent_prompt(debate_state: &DebateState, proposition: &str, human_name: &str, human_argument: &str) -> String {
    let mut prompt = format!(
        "You are a skilled debater arguing AGAINST the following proposition:\n\n\
         \"{}\"\n\n\
         Your opponent {} is arguing FOR this proposition.\n\n",
        proposition, human_name
    );

    // Add debate history
    if !debate_state.history.is_empty() {
        prompt.push_str("Previous exchanges:\n");
        for (speaker, msg) in &debate_state.history {
            if speaker == "Opponent" {
                prompt.push_str(&format!("You: {}\n\n", msg));
            } else {
                prompt.push_str(&format!("{}: {}\n\n", speaker, msg));
            }
        }
    }

    prompt.push_str(&format!(
        "{} just said:\n\"{}\"\n\n\
         Respond with a compelling counter-argument. Be concise but persuasive. \
         Address their specific points and make your case clearly.",
        human_name, human_argument
    ));

    prompt
}

fn build_judge_prompt(debate_state: &DebateState) -> String {
    let proposition = debate_state.proposition.as_deref().unwrap_or("the given topic");

    let mut prompt = format!(
        "You are an impartial judge evaluating a debate on the following proposition:\n\n\
         \"{}\"\n\n\
         Here is the complete debate:\n\n",
        proposition
    );

    for (speaker, msg) in &debate_state.history {
        let role = if speaker == "Opponent" {
            "AGAINST".to_string()
        } else {
            format!("FOR ({})", speaker)
        };
        prompt.push_str(&format!("[{}]: {}\n\n", role, msg));
    }

    prompt.push_str(
        "Please evaluate this debate and declare a winner. Consider:\n\
         1. Strength of arguments and evidence\n\
         2. Logical reasoning and coherence\n\
         3. Effective rebuttals of opposing points\n\
         4. Overall persuasiveness\n\n\
         Provide a brief analysis and then clearly state who won the debate and why."
    );

    prompt
}

async fn get_or_load_model(
    models: ModelCache,
    zulip: Arc<ZulipClient>,
    channel: &str,
    cache_key: &str,
    config: &ModelConfig,
) -> ModelLoadResult {
    // Check current state
    {
        let models_read = models.read().await;
        if let Some(state) = models_read.get(cache_key) {
            match state {
                ModelState::Ready(engine) => {
                    debug!("Using cached model for '{}'", cache_key);
                    return ModelLoadResult::Ready(engine.clone());
                }
                ModelState::Loading => {
                    debug!("Model for '{}' is still loading", cache_key);
                    return ModelLoadResult::AlreadyLoading;
                }
                ModelState::Failed(err) => {
                    debug!("Model for '{}' previously failed: {}", cache_key, err);
                    return ModelLoadResult::Failed(err.clone());
                }
            }
        }
    }

    // Mark as loading
    {
        let mut models_write = models.write().await;
        models_write.insert(cache_key.to_string(), ModelState::Loading);
    }

    // Notify user that we're downloading
    info!(
        "Starting model download for '{}': {} {}",
        cache_key, config.model_id, config.quantization
    );
    let _ = zulip
        .send_message(
            channel,
            cache_key.trim_end_matches("_judge"),
            &format!(
                "Downloading model **{}** ({})... I'll respond once it's ready.",
                config.model_id, config.quantization
            ),
        )
        .await;

    // Spawn the download in background
    let models_clone = models.clone();
    let cache_key_owned = cache_key.to_string();
    let config_clone = config.clone();
    tokio::spawn(async move {
        debug!(
            "Background model loading started for '{}'",
            cache_key_owned
        );

        let result = LlmEngine::new(
            &config_clone.model_id,
            &config_clone.quantization,
            config_clone.max_tokens,
            config_clone.temperature,
        )
        .await;

        let mut models_write = models_clone.write().await;
        match result {
            Ok(engine) => {
                info!("Model loaded successfully for '{}'", cache_key_owned);
                models_write.insert(cache_key_owned, ModelState::Ready(Arc::new(engine)));
            }
            Err(e) => {
                error!("Model loading failed for '{}': {}", cache_key_owned, e);
                models_write.insert(cache_key_owned, ModelState::Failed(e.to_string()));
            }
        }
    });

    ModelLoadResult::StartedLoading
}
