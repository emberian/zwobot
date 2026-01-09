//! Rhai scripting engine integration for zwobot games
//!
//! This crate provides a scriptable game engine that allows writing bot games
//! in Rhai scripts with full access to LLM inference and bot framework capabilities.

mod engine;
mod script;
mod loader;
mod session;
mod bridge;
pub mod api;
pub mod persistence;
pub mod shared_state;
pub mod timer;

pub use engine::RhaiEngine;
pub use script::{GameScript, ScriptMeta, CommandMeta, OptionMeta};
pub use loader::ScriptLoader;
pub use session::GameSession;
pub use bridge::{AsyncBridge, AsyncRequest, LlmConfig, ImageConfig};
pub use persistence::PersistenceManager;
pub use shared_state::SharedState;
pub use timer::TimerManager;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tracing::{info, debug, error};

/// Error types for the rhai-games crate
#[derive(Debug, thiserror::Error)]
pub enum RhaiError {
    #[error("Script parse error in {0}: {1}")]
    Parse(String, String),

    #[error("Script runtime error: {0}")]
    Runtime(String),

    #[error("Script not found: {0}")]
    ScriptNotFound(String),

    #[error("Function not found: {0}")]
    FunctionNotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Watch error: {0}")]
    Watch(String),

    #[error("LLM error: {0}")]
    Llm(String),

    #[error("Session error: {0}")]
    Session(String),

    #[error("Channel error: {0}")]
    Channel(String),
}

/// Result type for rhai-games operations
pub type Result<T> = std::result::Result<T, RhaiError>;

/// A timer response that needs to be sent to a channel
#[derive(Debug)]
pub struct TimerResponse {
    /// The topic/thread where the response should be sent
    pub topic: String,
    /// The channel/stream name (if known from session)
    pub channel: Option<String>,
    /// The response content
    pub response: api::ScriptResponse,
}

/// Main integration type that holds all Rhai game state
pub struct RhaiGamesData {
    /// Script loader with hot reload
    pub loader: Arc<ScriptLoader>,
    /// Rhai engine instance
    pub engine: Arc<RhaiEngine>,
    /// Active sessions by topic
    pub sessions: Arc<RwLock<HashMap<String, GameSession>>>,
    /// Async bridge sender for LLM calls
    bridge_tx: mpsc::Sender<AsyncRequest>,
    /// Timer manager
    pub timers: Arc<TimerManager>,
    /// Channel for timer responses that need to be sent
    timer_response_tx: mpsc::Sender<TimerResponse>,
    /// Persistence manager
    pub persistence: Arc<PersistenceManager>,
    /// Shared state manager
    pub shared_state: Arc<SharedState>,
}

impl RhaiGamesData {
    /// Create a new RhaiGamesData instance
    /// Returns the data and a receiver for timer responses that need to be sent
    pub async fn new(scripts_dir: impl Into<PathBuf>) -> Result<(Self, mpsc::Receiver<TimerResponse>)> {
        Self::with_data_dir(scripts_dir, "data/rhai-games").await
    }

    /// Create with custom data directory
    /// Returns the data and a receiver for timer responses that need to be sent
    pub async fn with_data_dir(
        scripts_dir: impl Into<PathBuf>,
        data_dir: impl Into<PathBuf>,
    ) -> Result<(Self, mpsc::Receiver<TimerResponse>)> {
        let scripts_dir = scripts_dir.into();
        let data_dir = data_dir.into();

        // Create persistence manager
        let persistence = Arc::new(PersistenceManager::new(&data_dir));

        // Create async bridge with persistence
        let (bridge, bridge_tx) = AsyncBridge::new(persistence.clone());

        // Create engine with bridge
        let engine = Arc::new(RhaiEngine::new(bridge_tx.clone()));

        // Create script loader
        let loader = Arc::new(ScriptLoader::new(scripts_dir));

        // Create timer manager
        let timers = Arc::new(TimerManager::new());

        // Create timer response channel
        let (timer_response_tx, timer_response_rx) = mpsc::channel(64);

        // Create shared state manager with persistence
        let shared_state = Arc::new(SharedState::new(Some(persistence.clone())));

        // Start async bridge in background
        tokio::spawn(bridge.run());

        // Load persisted shared state
        if let Err(e) = shared_state.load_persisted().await {
            debug!("Could not load persisted shared state: {}", e);
        }

        // Load all scripts
        let count = loader.load_all(&engine).await?;
        info!("Loaded {} Rhai game scripts", count);

        let data = Self {
            loader,
            engine,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            bridge_tx,
            timers,
            timer_response_tx,
            persistence,
            shared_state,
        };

        Ok((data, timer_response_rx))
    }

    /// Start the timer tick loop and shared state auto-save (should be called once)
    pub fn start_timer_loop(self: &Arc<Self>) {
        // Start timer tick loop
        let data = Arc::clone(self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(100));
            loop {
                interval.tick().await;
                if let Err(e) = data.process_timers().await {
                    error!("Timer processing error: {}", e);
                }
            }
        });

        // Start shared state auto-save loop
        self.shared_state.start_auto_save();
    }

    /// Process ready timers
    async fn process_timers(&self) -> Result<()> {
        let ready_timers = self.timers.collect_ready().await;

        for timer in ready_timers {
            // Check if session still exists before firing
            if !self.sessions.read().await.contains_key(&timer.topic) {
                debug!(
                    "Timer {} fired but session {} no longer exists, skipping",
                    timer.id, timer.topic
                );
                continue;
            }

            debug!("Firing timer {} for topic {}", timer.id, timer.topic);

            // Get the script for this timer
            let script = match self.loader.get(&timer.script_id).await {
                Some(s) => s,
                None => {
                    debug!("Script {} not found for timer", timer.script_id);
                    continue;
                }
            };

            // Check if script has on_timer handler
            let has_handler = script.ast.iter_functions().any(|f| f.name == "on_timer");
            if !has_handler {
                debug!("Script {} has no on_timer handler", timer.script_id);
                continue;
            }

            // Create minimal execution context for timer callback
            let ctx = api::ExecutionContext {
                topic: timer.topic.clone(),
                channel: String::new(),
                sender_name: String::new(),
                sender_id: 0,
                content: String::new(),
                args: {
                    let mut args = HashMap::new();
                    args.insert("timer_id".to_string(), timer.id.to_string());
                    // Store context as JSON string for script to parse
                    if let Ok(json) = serde_json::to_string(&persistence::dynamic_to_json(&timer.context)) {
                        args.insert("timer_context".to_string(), json);
                    }
                    args
                },
            };

            // Get channel from session if available
            let channel = self.sessions.read().await
                .get(&timer.topic)
                .and_then(|s| s.channel.clone());

            // Execute the on_timer handler
            match self.engine.call_script_fn(
                &script,
                &self.sessions,
                &timer.topic,
                "on_timer",
                &ctx,
                &self.bridge_tx,
            ).await {
                Ok(response) => {
                    if let Some(resp) = response {
                        debug!("Timer {} produced response: {:?}", timer.id, resp.content);
                        // Send response through channel for the bot to deliver
                        let timer_resp = TimerResponse {
                            topic: timer.topic.clone(),
                            channel,
                            response: resp,
                        };
                        if let Err(e) = self.timer_response_tx.send(timer_resp).await {
                            error!("Failed to send timer response: {}", e);
                        }
                    }
                }
                Err(e) => {
                    error!("Timer {} handler error: {}", timer.id, e);
                }
            }
        }

        Ok(())
    }

    /// Get all commands from all loaded scripts (for registration)
    pub async fn all_commands(&self) -> Vec<(String, CommandMeta)> {
        let scripts = self.loader.all().await;
        scripts
            .iter()
            .flat_map(|s| {
                s.meta
                    .commands
                    .iter()
                    .map(|c| (s.id.clone(), c.clone()))
            })
            .collect()
    }

    /// Check if there's an active session for a topic
    pub async fn has_session(&self, topic: &str) -> bool {
        self.sessions.read().await.contains_key(topic)
    }

    /// Get the script ID for an active session
    pub async fn session_script_id(&self, topic: &str) -> Option<String> {
        self.sessions
            .read()
            .await
            .get(topic)
            .map(|s| s.script_id.clone())
    }

    /// Start a new game session for a topic
    pub async fn start_session(&self, topic: &str, channel: Option<&str>, script_id: &str) -> Result<()> {
        // Verify script exists and get namespace
        let script = self.loader.get(script_id).await
            .ok_or_else(|| RhaiError::ScriptNotFound(script_id.to_string()))?;

        let namespace = script.meta.namespace.clone();

        let mut sessions = self.sessions.write().await;
        sessions.insert(
            topic.to_string(),
            GameSession::with_channel(script_id.to_string(), channel.map(String::from), namespace.clone()),
        );
        info!(
            "Started session for topic '{}' with script '{}' (namespace: {:?})",
            topic, script_id, namespace
        );
        Ok(())
    }

    /// End a game session
    pub async fn end_session(&self, topic: &str) {
        // Cancel all timers for this topic
        self.timers.cancel_all_for_topic(topic).await;

        let mut sessions = self.sessions.write().await;
        if sessions.remove(topic).is_some() {
            info!("Ended session for topic '{}'", topic);
        }
    }

    /// Execute a script command
    pub async fn execute_command(
        &self,
        script_id: &str,
        command: &str,
        args: &HashMap<String, String>,
        ctx: &api::ExecutionContext,
    ) -> Result<Option<api::ScriptResponse>> {
        // Get script
        let script = self.loader.get(script_id).await
            .ok_or_else(|| RhaiError::ScriptNotFound(script_id.to_string()))?;

        // Get or create session with channel and namespace
        let topic = ctx.topic.clone();
        let channel = ctx.channel.clone();
        {
            let mut sessions = self.sessions.write().await;
            sessions
                .entry(topic.clone())
                .or_insert_with(|| {
                    GameSession::with_channel(
                        script_id.to_string(),
                        Some(channel.clone()),
                        script.meta.namespace.clone(),
                    )
                });
        }

        // Build full context with args
        let mut full_ctx = ctx.clone();
        full_ctx.args = args.clone();

        // Execute command handler
        let fn_name = format!("on_command_{}", command);
        let result = self.engine.call_script_fn(
            &script,
            &self.sessions,
            &topic,
            &fn_name,
            &full_ctx,
            &self.bridge_tx,
        ).await?;

        // Process scheduled timers from script execution
        self.process_scheduled_timers(&topic, script_id).await;

        Ok(result)
    }

    /// Handle a message for an active session
    pub async fn handle_message(
        &self,
        topic: &str,
        ctx: &api::ExecutionContext,
    ) -> Result<Option<api::ScriptResponse>> {
        // Check if there's an active session
        let script_id = {
            let sessions = self.sessions.read().await;
            sessions.get(topic).map(|s| s.script_id.clone())
        };

        let script_id = match script_id {
            Some(id) => id,
            None => return Ok(None), // No active session
        };

        // Get script
        let script = self.loader.get(&script_id).await
            .ok_or_else(|| RhaiError::ScriptNotFound(script_id.clone()))?;

        // Check if script handles messages
        if !script.meta.handles_messages {
            return Ok(None);
        }

        // Execute message handler
        let result = self.engine.call_script_fn(
            &script,
            &self.sessions,
            topic,
            "on_message",
            ctx,
            &self.bridge_tx,
        ).await?;

        // Process scheduled timers from script execution
        self.process_scheduled_timers(topic, &script_id).await;

        Ok(result)
    }

    /// Process timers scheduled during script execution
    async fn process_scheduled_timers(&self, topic: &str, script_id: &str) {
        // Get scheduled timers from thread-local
        let scheduled = RhaiEngine::take_scheduled_timers();
        let cancelled = RhaiEngine::take_cancelled_timers();

        // Cancel requested timers
        for timer_id in cancelled {
            self.timers.cancel(timer_id).await;
        }

        // Schedule new timers
        for (_temp_id, delay, context, repeating) in scheduled {
            let real_id = self.timers.schedule(
                topic.to_string(),
                script_id.to_string(),
                delay,
                context,
                repeating,
            ).await;

            // Track timer in session
            let mut sessions = self.sessions.write().await;
            if let Some(session) = sessions.get_mut(topic) {
                session.add_timer(real_id);
            }
        }
    }

    /// Reload all scripts
    pub async fn reload(&self) -> Result<usize> {
        self.loader.reload_all(&self.engine).await
    }

    /// Get list of all loaded scripts
    pub async fn list_scripts(&self) -> Vec<(String, String, String)> {
        self.loader.all().await
            .iter()
            .map(|s| (s.id.clone(), s.meta.title.clone(), s.meta.description.clone()))
            .collect()
    }
}
