//! Async bridge for LLM calls, HTTP requests, and persistence
//!
//! Rhai is synchronous, but LLM inference, HTTP, and file I/O are async.
//! This bridge allows scripts to make blocking calls that are processed
//! asynchronously by the tokio runtime.

use rhai::Dynamic;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, info, warn};

use crate::persistence::PersistenceManager;

/// HTTP request timeout
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

/// Maximum HTTP response body size (1MB)
const HTTP_MAX_BODY_SIZE: usize = 1_000_000;

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
    // ---- LLM ----
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

    // ---- HTTP ----
    /// HTTP GET request
    HttpGet {
        url: String,
        headers: HashMap<String, String>,
        response: oneshot::Sender<Result<String, String>>,
    },
    /// HTTP POST request
    HttpPost {
        url: String,
        headers: HashMap<String, String>,
        body: String,
        content_type: String,
        response: oneshot::Sender<Result<String, String>>,
    },

    // ---- Persistence ----
    /// Save data to persistence
    PersistSave {
        namespace: String,
        key: String,
        value: Dynamic,
        response: oneshot::Sender<Result<(), String>>,
    },
    /// Load data from persistence
    PersistLoad {
        namespace: String,
        key: String,
        response: oneshot::Sender<Result<Dynamic, String>>,
    },
    /// List keys in a namespace
    PersistList {
        namespace: String,
        response: oneshot::Sender<Result<Vec<String>, String>>,
    },
    /// Delete a key from persistence
    PersistDelete {
        namespace: String,
        key: String,
        response: oneshot::Sender<Result<bool, String>>,
    },
    /// Check if a key exists
    PersistExists {
        namespace: String,
        key: String,
        response: oneshot::Sender<bool>,
    },
}

/// Bridge between sync Rhai execution and async bot operations
pub struct AsyncBridge {
    rx: mpsc::Receiver<AsyncRequest>,
    llm_cache: HashMap<String, Arc<llm::LlmEngine>>,
    http_client: reqwest::Client,
    persistence: Arc<PersistenceManager>,
}

impl AsyncBridge {
    /// Create a new async bridge and return the sender for making requests
    pub fn new(persistence: Arc<PersistenceManager>) -> (Self, mpsc::Sender<AsyncRequest>) {
        let (tx, rx) = mpsc::channel(32);
        let http_client = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .build()
            .expect("Failed to create HTTP client");

        (
            Self {
                rx,
                llm_cache: HashMap::new(),
                http_client,
                persistence,
            },
            tx,
        )
    }

    /// Run the bridge, processing async requests
    pub async fn run(mut self) {
        info!("AsyncBridge started");
        while let Some(request) = self.rx.recv().await {
            match request {
                // LLM requests
                AsyncRequest::GenerateLlm {
                    config,
                    prompt,
                    response,
                } => {
                    let result = self.handle_llm_generate(config, prompt).await;
                    let _ = response.send(result);
                }
                AsyncRequest::CompleteLlm {
                    config,
                    prompt,
                    response,
                } => {
                    let result = self.handle_llm_complete(config, prompt).await;
                    let _ = response.send(result);
                }

                // HTTP requests
                AsyncRequest::HttpGet {
                    url,
                    headers,
                    response,
                } => {
                    let result = self.handle_http_get(url, headers).await;
                    let _ = response.send(result);
                }
                AsyncRequest::HttpPost {
                    url,
                    headers,
                    body,
                    content_type,
                    response,
                } => {
                    let result = self.handle_http_post(url, headers, body, content_type).await;
                    let _ = response.send(result);
                }

                // Persistence requests
                AsyncRequest::PersistSave {
                    namespace,
                    key,
                    value,
                    response,
                } => {
                    let result = self.persistence.save(&namespace, &key, value).await;
                    let _ = response.send(result);
                }
                AsyncRequest::PersistLoad {
                    namespace,
                    key,
                    response,
                } => {
                    let result = self.persistence.load(&namespace, &key).await;
                    let _ = response.send(result);
                }
                AsyncRequest::PersistList {
                    namespace,
                    response,
                } => {
                    let result = self.persistence.list(&namespace).await;
                    let _ = response.send(result);
                }
                AsyncRequest::PersistDelete {
                    namespace,
                    key,
                    response,
                } => {
                    let result = self.persistence.delete(&namespace, &key).await;
                    let _ = response.send(result);
                }
                AsyncRequest::PersistExists {
                    namespace,
                    key,
                    response,
                } => {
                    let exists = self.persistence.exists(&namespace, &key).await;
                    let _ = response.send(exists);
                }
            }
        }
        info!("AsyncBridge stopped");
    }

    // ---- LLM handlers ----

    async fn handle_llm_generate(
        &mut self,
        config: LlmConfig,
        prompt: String,
    ) -> Result<String, String> {
        let cache_key = format!("{}:{}", config.model_id, config.quantization);

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

        engine.generate(&prompt).await.map_err(|e| e.to_string())
    }

    async fn handle_llm_complete(
        &mut self,
        config: LlmConfig,
        prompt: String,
    ) -> Result<String, String> {
        let cache_key = format!("{}:{}", config.model_id, config.quantization);

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

        engine.complete(&prompt).await.map_err(|e| e.to_string())
    }

    // ---- HTTP handlers ----

    async fn handle_http_get(
        &self,
        url: String,
        headers: HashMap<String, String>,
    ) -> Result<String, String> {
        debug!("HTTP GET: {}", url);

        let mut request = self.http_client.get(&url);
        for (k, v) in headers {
            request = request.header(&k, &v);
        }

        let response = request.send().await.map_err(|e| e.to_string())?;

        if !response.status().is_success() {
            return Err(format!("HTTP {}", response.status()));
        }

        let body = response.text().await.map_err(|e| e.to_string())?;

        if body.len() > HTTP_MAX_BODY_SIZE {
            warn!("HTTP response truncated from {} bytes", body.len());
            return Ok(body[..HTTP_MAX_BODY_SIZE].to_string());
        }

        Ok(body)
    }

    async fn handle_http_post(
        &self,
        url: String,
        headers: HashMap<String, String>,
        body: String,
        content_type: String,
    ) -> Result<String, String> {
        debug!("HTTP POST: {}", url);

        let mut request = self
            .http_client
            .post(&url)
            .header("Content-Type", content_type)
            .body(body);

        for (k, v) in headers {
            request = request.header(&k, &v);
        }

        let response = request.send().await.map_err(|e| e.to_string())?;

        if !response.status().is_success() {
            return Err(format!("HTTP {}", response.status()));
        }

        let body = response.text().await.map_err(|e| e.to_string())?;

        if body.len() > HTTP_MAX_BODY_SIZE {
            warn!("HTTP response truncated from {} bytes", body.len());
            return Ok(body[..HTTP_MAX_BODY_SIZE].to_string());
        }

        Ok(body)
    }
}

// ---- Blocking wrappers for sync Rhai context ----

/// Make a blocking LLM generate call
pub fn blocking_llm_generate(
    bridge_tx: &mpsc::Sender<AsyncRequest>,
    config: LlmConfig,
    prompt: String,
) -> Result<String, String> {
    let (tx, rx) = oneshot::channel();
    bridge_tx
        .blocking_send(AsyncRequest::GenerateLlm {
            config,
            prompt,
            response: tx,
        })
        .map_err(|e| format!("Failed to send request: {}", e))?;
    rx.blocking_recv()
        .map_err(|e| format!("Failed to receive response: {}", e))?
}

/// Make a blocking LLM complete call
pub fn blocking_llm_complete(
    bridge_tx: &mpsc::Sender<AsyncRequest>,
    config: LlmConfig,
    prompt: String,
) -> Result<String, String> {
    let (tx, rx) = oneshot::channel();
    bridge_tx
        .blocking_send(AsyncRequest::CompleteLlm {
            config,
            prompt,
            response: tx,
        })
        .map_err(|e| format!("Failed to send request: {}", e))?;
    rx.blocking_recv()
        .map_err(|e| format!("Failed to receive response: {}", e))?
}

/// Make a blocking HTTP GET call
pub fn blocking_http_get(
    bridge_tx: &mpsc::Sender<AsyncRequest>,
    url: String,
    headers: HashMap<String, String>,
) -> Result<String, String> {
    let (tx, rx) = oneshot::channel();
    bridge_tx
        .blocking_send(AsyncRequest::HttpGet {
            url,
            headers,
            response: tx,
        })
        .map_err(|e| format!("Failed to send request: {}", e))?;
    rx.blocking_recv()
        .map_err(|e| format!("Failed to receive response: {}", e))?
}

/// Make a blocking HTTP POST call
pub fn blocking_http_post(
    bridge_tx: &mpsc::Sender<AsyncRequest>,
    url: String,
    headers: HashMap<String, String>,
    body: String,
    content_type: String,
) -> Result<String, String> {
    let (tx, rx) = oneshot::channel();
    bridge_tx
        .blocking_send(AsyncRequest::HttpPost {
            url,
            headers,
            body,
            content_type,
            response: tx,
        })
        .map_err(|e| format!("Failed to send request: {}", e))?;
    rx.blocking_recv()
        .map_err(|e| format!("Failed to receive response: {}", e))?
}

/// Make a blocking persistence save call
pub fn blocking_persist_save(
    bridge_tx: &mpsc::Sender<AsyncRequest>,
    namespace: String,
    key: String,
    value: Dynamic,
) -> Result<(), String> {
    let (tx, rx) = oneshot::channel();
    bridge_tx
        .blocking_send(AsyncRequest::PersistSave {
            namespace,
            key,
            value,
            response: tx,
        })
        .map_err(|e| format!("Failed to send request: {}", e))?;
    rx.blocking_recv()
        .map_err(|e| format!("Failed to receive response: {}", e))?
}

/// Make a blocking persistence load call
pub fn blocking_persist_load(
    bridge_tx: &mpsc::Sender<AsyncRequest>,
    namespace: String,
    key: String,
) -> Result<Dynamic, String> {
    let (tx, rx) = oneshot::channel();
    bridge_tx
        .blocking_send(AsyncRequest::PersistLoad {
            namespace,
            key,
            response: tx,
        })
        .map_err(|e| format!("Failed to send request: {}", e))?;
    rx.blocking_recv()
        .map_err(|e| format!("Failed to receive response: {}", e))?
}

/// Make a blocking persistence list call
pub fn blocking_persist_list(
    bridge_tx: &mpsc::Sender<AsyncRequest>,
    namespace: String,
) -> Result<Vec<String>, String> {
    let (tx, rx) = oneshot::channel();
    bridge_tx
        .blocking_send(AsyncRequest::PersistList {
            namespace,
            response: tx,
        })
        .map_err(|e| format!("Failed to send request: {}", e))?;
    rx.blocking_recv()
        .map_err(|e| format!("Failed to receive response: {}", e))?
}

/// Make a blocking persistence delete call
pub fn blocking_persist_delete(
    bridge_tx: &mpsc::Sender<AsyncRequest>,
    namespace: String,
    key: String,
) -> Result<bool, String> {
    let (tx, rx) = oneshot::channel();
    bridge_tx
        .blocking_send(AsyncRequest::PersistDelete {
            namespace,
            key,
            response: tx,
        })
        .map_err(|e| format!("Failed to send request: {}", e))?;
    rx.blocking_recv()
        .map_err(|e| format!("Failed to receive response: {}", e))?
}

/// Make a blocking persistence exists call
pub fn blocking_persist_exists(
    bridge_tx: &mpsc::Sender<AsyncRequest>,
    namespace: String,
    key: String,
) -> bool {
    let (tx, rx) = oneshot::channel();
    if bridge_tx
        .blocking_send(AsyncRequest::PersistExists {
            namespace,
            key,
            response: tx,
        })
        .is_err()
    {
        return false;
    }
    rx.blocking_recv().unwrap_or(false)
}
