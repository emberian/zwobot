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
use tracing::{debug, info};

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

/// Configuration for image generation
#[derive(Clone, Debug)]
pub struct ImageConfig {
    pub model_id: String,
    pub offloaded: bool,
    pub width: usize,
    pub height: usize,
}

impl Default for ImageConfig {
    fn default() -> Self {
        Self {
            model_id: "black-forest-labs/FLUX.1-schnell".to_string(),
            offloaded: true,
            width: 512,
            height: 512,
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

    // ---- Image Generation ----
    /// Generate an image from a text prompt
    GenerateImage {
        config: ImageConfig,
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
    image_cache: HashMap<String, Arc<llm::ImageEngine>>,
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
                image_cache: HashMap::new(),
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

                // Image generation
                AsyncRequest::GenerateImage {
                    config,
                    prompt,
                    response,
                } => {
                    let result = self.handle_image_generate(config, prompt).await;
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

    // ---- Image generation handler ----

    async fn handle_image_generate(
        &mut self,
        config: ImageConfig,
        prompt: String,
    ) -> Result<String, String> {
        let cache_key = format!("{}:{}x{}", config.model_id, config.width, config.height);

        let engine = if let Some(engine) = self.image_cache.get(&cache_key) {
            engine.clone()
        } else {
            info!(
                "Loading image model {} ({}x{}, offloaded: {})",
                config.model_id, config.width, config.height, config.offloaded
            );

            let engine = llm::ImageEngine::new(
                &config.model_id,
                config.offloaded,
                config.width,
                config.height,
            )
            .await
            .map_err(|e| e.to_string())?;

            let engine = Arc::new(engine);
            self.image_cache.insert(cache_key, engine.clone());
            engine
        };

        engine.generate(&prompt).await.map_err(|e| e.to_string())
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
            return Err(format!(
                "HTTP response too large: {} bytes (max: {})",
                body.len(),
                HTTP_MAX_BODY_SIZE
            ));
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
            return Err(format!(
                "HTTP response too large: {} bytes (max: {})",
                body.len(),
                HTTP_MAX_BODY_SIZE
            ));
        }

        Ok(body)
    }
}

// ---- Blocking wrappers for sync Rhai context ----

/// Macro to generate blocking wrapper functions for AsyncRequest variants.
/// Reduces boilerplate by handling the channel setup and error handling uniformly.
macro_rules! blocking_request {
    // For Result<T, String> return types
    ($fn_name:ident, $variant:ident { $($field:ident : $ty:ty),* $(,)? } -> $ret:ty) => {
        pub fn $fn_name(
            bridge_tx: &mpsc::Sender<AsyncRequest>,
            $($field: $ty),*
        ) -> Result<$ret, String> {
            let (tx, rx) = oneshot::channel();
            bridge_tx
                .blocking_send(AsyncRequest::$variant {
                    $($field,)*
                    response: tx,
                })
                .map_err(|e| format!("Failed to send request: {}", e))?;
            rx.blocking_recv()
                .map_err(|e| format!("Failed to receive response: {}", e))?
        }
    };
    // For direct return types (unwraps the Result)
    ($fn_name:ident, $variant:ident { $($field:ident : $ty:ty),* $(,)? } => $ret:ty, $default:expr) => {
        pub fn $fn_name(
            bridge_tx: &mpsc::Sender<AsyncRequest>,
            $($field: $ty),*
        ) -> $ret {
            let (tx, rx) = oneshot::channel();
            if bridge_tx
                .blocking_send(AsyncRequest::$variant {
                    $($field,)*
                    response: tx,
                })
                .is_err()
            {
                return $default;
            }
            rx.blocking_recv().unwrap_or($default)
        }
    };
}

// LLM operations
blocking_request!(blocking_llm_generate, GenerateLlm { config: LlmConfig, prompt: String } -> String);
blocking_request!(blocking_llm_complete, CompleteLlm { config: LlmConfig, prompt: String } -> String);

// Image generation
blocking_request!(blocking_image_generate, GenerateImage { config: ImageConfig, prompt: String } -> String);

// HTTP operations
blocking_request!(blocking_http_get, HttpGet { url: String, headers: HashMap<String, String> } -> String);
blocking_request!(blocking_http_post, HttpPost { url: String, headers: HashMap<String, String>, body: String, content_type: String } -> String);

// Persistence operations
blocking_request!(blocking_persist_save, PersistSave { namespace: String, key: String, value: Dynamic } -> ());
blocking_request!(blocking_persist_load, PersistLoad { namespace: String, key: String } -> Dynamic);
blocking_request!(blocking_persist_list, PersistList { namespace: String } -> Vec<String>);
blocking_request!(blocking_persist_delete, PersistDelete { namespace: String, key: String } -> bool);
blocking_request!(blocking_persist_exists, PersistExists { namespace: String, key: String } => bool, false);
