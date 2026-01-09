//! Async bridge for LLM calls and other async operations
//!
//! Rhai is synchronous, but LLM inference is async. This bridge
//! allows scripts to make blocking calls that are processed
//! asynchronously by the tokio runtime.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tracing::info;

/// Configuration for LLM inference
#[derive(Clone, Debug)]
pub struct LlmConfig {
    pub model_id: String,
    pub quantization: String,
    pub max_tokens: u32,
    pub temperature: f32,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            model_id: String::new(),
            quantization: "Q4_K_M".to_string(),
            max_tokens: 1024,
            temperature: 0.7,
        }
    }
}

/// Requests that need async execution
pub enum AsyncRequest {
    /// Generate text using LLM (chat completion with template)
    GenerateLlm {
        config: LlmConfig,
        prompt: String,
        response: oneshot::Sender<Result<String, String>>,
    },
    /// Complete text using LLM (raw completion, no template)
    CompleteLlm {
        config: LlmConfig,
        prompt: String,
        response: oneshot::Sender<Result<String, String>>,
    },
}

/// Bridge between sync Rhai execution and async bot operations
pub struct AsyncBridge {
    rx: mpsc::Receiver<AsyncRequest>,
    llm_cache: HashMap<String, Arc<llm::LlmEngine>>,
}

impl AsyncBridge {
    /// Create a new async bridge and return the sender for making requests
    pub fn new() -> (Self, mpsc::Sender<AsyncRequest>) {
        let (tx, rx) = mpsc::channel(32);
        (
            Self {
                rx,
                llm_cache: HashMap::new(),
            },
            tx,
        )
    }

    /// Run the bridge, processing async requests
    pub async fn run(mut self) {
        info!("AsyncBridge started");
        while let Some(request) = self.rx.recv().await {
            match request {
                AsyncRequest::GenerateLlm { config, prompt, response } => {
                    let result = self.handle_llm_generate(config, prompt).await;
                    let _ = response.send(result);
                }
                AsyncRequest::CompleteLlm { config, prompt, response } => {
                    let result = self.handle_llm_complete(config, prompt).await;
                    let _ = response.send(result);
                }
            }
        }
        info!("AsyncBridge stopped");
    }

    /// Handle an LLM generation request
    async fn handle_llm_generate(
        &mut self,
        config: LlmConfig,
        prompt: String,
    ) -> Result<String, String> {
        let cache_key = format!("{}:{}", config.model_id, config.quantization);

        // Get or create engine
        let engine = if let Some(engine) = self.llm_cache.get(&cache_key) {
            engine.clone()
        } else {
            info!(
                "Loading LLM model {} ({})",
                config.model_id, config.quantization
            );

            let engine = llm::LlmEngine::new(
                &config.model_id,
                &config.quantization,
                config.max_tokens,
                config.temperature,
            )
            .await
            .map_err(|e| e.to_string())?;

            let engine = Arc::new(engine);
            self.llm_cache.insert(cache_key, engine.clone());
            engine
        };

        // Generate (chat completion)
        engine.generate(&prompt).await.map_err(|e| e.to_string())
    }

    /// Handle an LLM raw completion request (no chat template)
    async fn handle_llm_complete(
        &mut self,
        config: LlmConfig,
        prompt: String,
    ) -> Result<String, String> {
        let cache_key = format!("{}:{}", config.model_id, config.quantization);

        // Get or create engine
        let engine = if let Some(engine) = self.llm_cache.get(&cache_key) {
            engine.clone()
        } else {
            info!(
                "Loading LLM model {} ({})",
                config.model_id, config.quantization
            );

            let engine = llm::LlmEngine::new(
                &config.model_id,
                &config.quantization,
                config.max_tokens,
                config.temperature,
            )
            .await
            .map_err(|e| e.to_string())?;

            let engine = Arc::new(engine);
            self.llm_cache.insert(cache_key, engine.clone());
            engine
        };

        // Complete (raw, no chat template)
        engine.complete(&prompt).await.map_err(|e| e.to_string())
    }
}

/// Make a blocking LLM call from sync Rhai context
///
/// This function is called from the sync Rhai execution context.
/// It sends a request to the async bridge and blocks until completion.
pub fn blocking_llm_generate(
    bridge_tx: &mpsc::Sender<AsyncRequest>,
    config: LlmConfig,
    prompt: String,
) -> Result<String, String> {
    let (tx, rx) = oneshot::channel();

    // Send the request
    // We need to use blocking send since we're in a sync context
    bridge_tx
        .blocking_send(AsyncRequest::GenerateLlm {
            config,
            prompt,
            response: tx,
        })
        .map_err(|e| format!("Failed to send LLM request: {}", e))?;

    // Block waiting for response
    rx.blocking_recv()
        .map_err(|e| format!("Failed to receive LLM response: {}", e))?
}

/// Make a blocking LLM raw completion call from sync Rhai context
///
/// This function is called from the sync Rhai execution context.
/// It sends a request to the async bridge and blocks until completion.
/// Unlike `blocking_llm_generate`, this does not apply a chat template.
pub fn blocking_llm_complete(
    bridge_tx: &mpsc::Sender<AsyncRequest>,
    config: LlmConfig,
    prompt: String,
) -> Result<String, String> {
    let (tx, rx) = oneshot::channel();

    // Send the request
    bridge_tx
        .blocking_send(AsyncRequest::CompleteLlm {
            config,
            prompt,
            response: tx,
        })
        .map_err(|e| format!("Failed to send LLM request: {}", e))?;

    // Block waiting for response
    rx.blocking_recv()
        .map_err(|e| format!("Failed to receive LLM response: {}", e))?
}
