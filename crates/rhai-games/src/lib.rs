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

pub use engine::RhaiEngine;
pub use script::{GameScript, ScriptMeta, CommandMeta, OptionMeta};
pub use loader::ScriptLoader;
pub use session::GameSession;
pub use bridge::{AsyncBridge, AsyncRequest, LlmConfig};

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::info;

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
}

impl RhaiGamesData {
    /// Create a new RhaiGamesData instance
    pub async fn new(scripts_dir: impl Into<PathBuf>) -> Result<Self> {
        let scripts_dir = scripts_dir.into();

        // Create async bridge
        let (bridge, bridge_tx) = AsyncBridge::new();

        // Create engine with bridge
        let engine = Arc::new(RhaiEngine::new(bridge_tx.clone()));

        // Create script loader
        let loader = Arc::new(ScriptLoader::new(scripts_dir));

        // Start async bridge in background
        tokio::spawn(bridge.run());

        // Load all scripts
        let count = loader.load_all(&engine).await?;
        info!("Loaded {} Rhai game scripts", count);

        Ok(Self {
            loader,
            engine,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            bridge_tx,
        })
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
    pub async fn start_session(&self, topic: &str, script_id: &str) -> Result<()> {
        // Verify script exists
        if self.loader.get(script_id).await.is_none() {
            return Err(RhaiError::ScriptNotFound(script_id.to_string()));
        }

        let mut sessions = self.sessions.write().await;
        sessions.insert(topic.to_string(), GameSession::new(script_id.to_string()));
        info!("Started session for topic '{}' with script '{}'", topic, script_id);
        Ok(())
    }

    /// End a game session
    pub async fn end_session(&self, topic: &str) {
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

        // Get or create session
        let topic = ctx.topic.clone();
        {
            let mut sessions = self.sessions.write().await;
            sessions
                .entry(topic.clone())
                .or_insert_with(|| GameSession::new(script_id.to_string()));
        }

        // Build full context with args
        let mut full_ctx = ctx.clone();
        full_ctx.args = args.clone();

        // Execute command handler
        let fn_name = format!("on_command_{}", command);
        self.engine.call_script_fn(&script, &self.sessions, &topic, &fn_name, &full_ctx, &self.bridge_tx).await
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
        self.engine.call_script_fn(&script, &self.sessions, topic, "on_message", ctx, &self.bridge_tx).await
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
