use crate::error::{ZwobotError, Result};
use hf_hub::api::sync::Api;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaChatMessage, LlamaModel, Special};
use llama_cpp_2::sampling::LlamaSampler;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::{debug, info, trace, warn};

/// Progress updates during model loading
#[derive(Debug, Clone)]
pub enum ModelLoadProgress {
    /// Starting to resolve/download the model
    Resolving { model_id: String },
    /// Download in progress (if from HuggingFace)
    Downloading { model_id: String, filename: String },
    /// Loading model into memory
    Loading { path: String },
    /// Model loaded successfully
    Loaded { vocab: i32, params: u64 },
    /// Loading failed
    Failed { error: String },
}

/// Thread-safe wrapper for llama.cpp inference
pub struct LlmEngine {
    inner: Arc<Mutex<LlmInner>>,
    max_tokens: u32,
    temperature: f32,
    /// Current model identifier
    model_id: Arc<Mutex<String>>,
    /// Current quantization level
    quantization: Arc<Mutex<String>>,
}

struct LlmInner {
    backend: LlamaBackend,
    model: LlamaModel,
}

/// Resolve model_id to a local GGUF file path.
/// If model_id is a local file, use it directly.
/// If it looks like a HF repo (org/model), download the GGUF from HuggingFace.
fn resolve_model_path(model_id: &str, quantization: &str) -> Result<PathBuf> {
    let path = PathBuf::from(model_id);

    // If it's already a local file, use it directly
    if path.exists() || model_id.ends_with(".gguf") {
        info!("Using local model file: {}", model_id);
        return Ok(path);
    }

    // Treat as HuggingFace repo - try to download GGUF
    if model_id.contains('/') {
        info!("Downloading model from HuggingFace: {}", model_id);

        let api = Api::new().map_err(|e| ZwobotError::ModelLoad(format!("HF API error: {e}")))?;

        // Check if repo already has -GGUF suffix to avoid duplication
        let (gguf_repo, base_repo) = if model_id.ends_with("-GGUF") {
            (model_id.to_string(), model_id.trim_end_matches("-GGUF").to_string())
        } else {
            (format!("{}-GGUF", model_id), model_id.to_string())
        };
        let quant_lower_underscore = quantization.to_lowercase(); // q8_0
        let quant_lower_hyphen = quant_lower_underscore.replace('_', "-"); // q8-0

        // Common GGUF filename patterns to try
        // Strip -GGUF from model name if present (for filename generation)
        let raw_model_name = model_id.split('/').last().unwrap_or("model");
        let model_name = raw_model_name.trim_end_matches("-GGUF");
        let model_name_lower = model_name.to_lowercase();
        // Convert decimal points to underscores (e.g., "1.7B" -> "1_7b")
        let model_name_underscore = model_name_lower.replace('.', "_");
        let quant_upper = quantization.to_uppercase().replace('-', "_");
        let filenames = [
            // Uppercase quant variants
            format!("{}-{}.gguf", model_name, quant_upper),
            format!("{}.{}.gguf", model_name, quant_upper),
            // Lowercase with underscore (most common on HF)
            format!("{}-{}.gguf", model_name_lower, quant_lower_underscore),
            format!("{}.{}.gguf", model_name_lower, quant_lower_underscore),
            // With decimal -> underscore conversion
            format!("{}-{}.gguf", model_name_underscore, quant_lower_underscore),
            format!("{}_{}.gguf", model_name_underscore, quant_lower_underscore),
            // Lowercase with hyphen
            format!("{}-{}.gguf", model_name, quant_lower_hyphen),
            format!("{}-{}.gguf", model_name_lower, quant_lower_hyphen),
            // Just quant name
            format!("{}.gguf", quant_lower_underscore),
            format!("{}.gguf", quant_lower_hyphen),
            // Common GGUF patterns
            format!("ggml-model-{}.gguf", quant_lower_underscore),
            format!("{}-gguf-{}.gguf", model_name_underscore, quant_lower_underscore),
        ];

        // Try GGUF repo first
        let repo = api.model(gguf_repo.clone());
        for filename in &filenames {
            debug!("Trying to download: {}/{}", gguf_repo, filename);
            if let Ok(path) = repo.get(filename) {
                info!("Downloaded model to: {}", path.display());
                return Ok(path);
            }
        }

        // Fall back to base repo (without -GGUF suffix)
        let repo = api.model(base_repo.clone());
        for filename in &filenames {
            debug!("Trying to download: {}/{}", base_repo, filename);
            if let Ok(path) = repo.get(filename) {
                info!("Downloaded model to: {}", path.display());
                return Ok(path);
            }
        }

        return Err(ZwobotError::ModelLoad(format!(
            "Could not find GGUF file in {} or {}. Tried filenames: {:?}",
            gguf_repo, base_repo, filenames
        )));
    }

    Err(ZwobotError::ModelLoad(format!(
        "Model not found: {}. Provide a local .gguf file path or HuggingFace repo (org/model)",
        model_id
    )))
}

/// Async version of resolve_model_path for use in model switching.
async fn resolve_model_path_with_progress(
    model_id: &str,
    quantization: &str,
    _progress_tx: mpsc::Sender<ModelLoadProgress>,
) -> Result<PathBuf> {
    let model_id = model_id.to_string();
    let quantization = quantization.to_string();

    tokio::task::spawn_blocking(move || {
        let path = PathBuf::from(&model_id);

        // If it's already a local file, use it directly
        if path.exists() || model_id.ends_with(".gguf") {
            info!("Using local model file: {}", model_id);
            return Ok(path);
        }

        // Treat as HuggingFace repo - try to download GGUF
        if model_id.contains('/') {
            info!("Downloading model from HuggingFace: {}", model_id);

            let api = Api::new().map_err(|e| ZwobotError::ModelLoad(format!("HF API error: {e}")))?;

            // Check if repo already has -GGUF suffix to avoid duplication
            let (gguf_repo, base_repo) = if model_id.ends_with("-GGUF") {
                (model_id.clone(), model_id.trim_end_matches("-GGUF").to_string())
            } else {
                (format!("{}-GGUF", model_id), model_id.clone())
            };
            let quant_lower_underscore = quantization.to_lowercase(); // q8_0
            let quant_lower_hyphen = quant_lower_underscore.replace('_', "-"); // q8-0

            // Common GGUF filename patterns to try
            // Strip -GGUF from model name if present (for filename generation)
            let raw_model_name = model_id.split('/').last().unwrap_or("model");
            let model_name = raw_model_name.trim_end_matches("-GGUF");
            let model_name_lower = model_name.to_lowercase();
            // Convert decimal points to underscores (e.g., "1.7B" -> "1_7b")
            let model_name_underscore = model_name_lower.replace('.', "_");
            let quant_upper = quantization.to_uppercase().replace('-', "_");
            let filenames = [
                // Uppercase quant variants
                format!("{}-{}.gguf", model_name, quant_upper),
                format!("{}.{}.gguf", model_name, quant_upper),
                // Lowercase with underscore (most common on HF)
                format!("{}-{}.gguf", model_name_lower, quant_lower_underscore),
                format!("{}.{}.gguf", model_name_lower, quant_lower_underscore),
                // With decimal -> underscore conversion
                format!("{}-{}.gguf", model_name_underscore, quant_lower_underscore),
                format!("{}_{}.gguf", model_name_underscore, quant_lower_underscore),
                // Lowercase with hyphen
                format!("{}-{}.gguf", model_name, quant_lower_hyphen),
                format!("{}-{}.gguf", model_name_lower, quant_lower_hyphen),
                // Just quant name
                format!("{}.gguf", quant_lower_underscore),
                format!("{}.gguf", quant_lower_hyphen),
                // Common GGUF patterns
                format!("ggml-model-{}.gguf", quant_lower_underscore),
                format!("{}-gguf-{}.gguf", model_name_underscore, quant_lower_underscore),
            ];

            // Try GGUF repo first
            let repo = api.model(gguf_repo.clone());
            for filename in &filenames {
                debug!("Trying to download: {}/{}", gguf_repo, filename);
                // Note: We can't easily send async progress from blocking context,
                // but logs will show the download progress
                if let Ok(path) = repo.get(filename) {
                    info!("Downloaded model to: {}", path.display());
                    return Ok(path);
                }
            }

            // Fall back to base repo (without -GGUF suffix)
            let repo = api.model(base_repo.clone());
            for filename in &filenames {
                debug!("Trying to download: {}/{}", base_repo, filename);
                if let Ok(path) = repo.get(filename) {
                    info!("Downloaded model to: {}", path.display());
                    return Ok(path);
                }
            }

            return Err(ZwobotError::ModelLoad(format!(
                "Could not find GGUF file in {} or {}. Tried filenames: {:?}",
                gguf_repo, base_repo, filenames
            )));
        }

        Err(ZwobotError::ModelLoad(format!(
            "Model not found: {}. Provide a local .gguf file path or HuggingFace repo (org/model)",
            model_id
        )))
    })
    .await
    .map_err(|e| ZwobotError::ModelLoad(format!("Task join error: {e}")))?
}

impl LlmEngine {
    pub async fn new(
        model_id: &str,
        quantization: &str,
        max_tokens: u32,
        temperature: f32,
    ) -> Result<Self> {
        info!("Initializing llama.cpp backend");

        let path = resolve_model_path(model_id, quantization)?;
        let model_id_owned = model_id.to_string();
        let quantization_owned = quantization.to_string();

        // Run blocking initialization in a separate thread
        let inner = tokio::task::spawn_blocking(move || {
            let backend = LlamaBackend::init()
                .map_err(|e| ZwobotError::ModelLoad(format!("Backend init failed: {e}")))?;

            info!("Loading model: {}", path.display());

            // GPU acceleration - offload all layers
            let model_params = LlamaModelParams::default().with_n_gpu_layers(1000);

            let model = LlamaModel::load_from_file(&backend, &path, &model_params)
                .map_err(|e| ZwobotError::ModelLoad(format!("Model load failed: {e}")))?;

            info!(
                "Model loaded: vocab={}, params={}",
                model.n_vocab(),
                model.n_params()
            );

            Ok::<_, ZwobotError>(LlmInner { backend, model })
        })
        .await
        .map_err(|e| ZwobotError::ModelLoad(format!("Task join error: {e}")))??;

        info!("Model loaded successfully");

        Ok(Self {
            inner: Arc::new(Mutex::new(inner)),
            max_tokens,
            temperature,
            model_id: Arc::new(Mutex::new(model_id_owned)),
            quantization: Arc::new(Mutex::new(quantization_owned)),
        })
    }

    /// Returns the current model ID.
    pub fn current_model_id(&self) -> String {
        self.model_id.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// Returns the current quantization level.
    pub fn current_quantization(&self) -> String {
        self.quantization.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// Attempts to switch to a new model, with progress reporting.
    /// Returns Ok(()) on success, or Err with the original model still loaded on failure.
    pub async fn switch_model(
        &self,
        new_model_id: &str,
        new_quantization: &str,
        progress_tx: mpsc::Sender<ModelLoadProgress>,
    ) -> Result<()> {
        let new_model_id_owned = new_model_id.to_string();
        let new_quantization_owned = new_quantization.to_string();

        // Send resolving progress
        let _ = progress_tx.send(ModelLoadProgress::Resolving {
            model_id: new_model_id_owned.clone(),
        }).await;

        // Resolve the path first (this may trigger download)
        let path = match resolve_model_path_with_progress(
            &new_model_id_owned,
            &new_quantization_owned,
            progress_tx.clone(),
        ).await {
            Ok(p) => p,
            Err(e) => {
                let _ = progress_tx.send(ModelLoadProgress::Failed {
                    error: e.to_string(),
                }).await;
                return Err(e);
            }
        };

        let path_str = path.display().to_string();
        let _ = progress_tx.send(ModelLoadProgress::Loading {
            path: path_str.clone(),
        }).await;

        // Load the new model in a blocking task, reusing the existing backend
        let progress_tx_clone = progress_tx.clone();
        let inner = self.inner.clone();
        let load_result = tokio::task::spawn_blocking(move || {
            // Lock and use the existing backend to load the new model
            let mut inner_guard = inner.lock()
                .map_err(|e| ZwobotError::ModelLoad(format!("Lock error: {e}")))?;

            info!("Loading new model: {}", path.display());

            let model_params = LlamaModelParams::default().with_n_gpu_layers(1000);

            let model = LlamaModel::load_from_file(&inner_guard.backend, &path, &model_params)
                .map_err(|e| ZwobotError::ModelLoad(format!("Model load failed: {e}")))?;

            let vocab = model.n_vocab();
            let params = model.n_params();

            info!("New model loaded: vocab={}, params={}", vocab, params);

            // Replace the model, keeping the same backend
            inner_guard.model = model;

            Ok::<_, ZwobotError>((vocab, params))
        })
        .await
        .map_err(|e| ZwobotError::ModelLoad(format!("Task join error: {e}")))?;

        match load_result {
            Ok((vocab, params)) => {

                // Update model info
                {
                    let mut model_id = self.model_id.lock()
                        .map_err(|e| ZwobotError::ModelLoad(format!("Lock error: {e}")))?;
                    *model_id = new_model_id_owned;
                }
                {
                    let mut quantization = self.quantization.lock()
                        .map_err(|e| ZwobotError::ModelLoad(format!("Lock error: {e}")))?;
                    *quantization = new_quantization_owned;
                }

                let _ = progress_tx.send(ModelLoadProgress::Loaded { vocab, params }).await;
                info!("Model switch completed successfully");
                Ok(())
            }
            Err(e) => {
                let _ = progress_tx_clone.send(ModelLoadProgress::Failed {
                    error: e.to_string(),
                }).await;
                warn!("Model switch failed, keeping original model: {}", e);
                Err(e)
            }
        }
    }

    pub async fn generate(&self, prompt: &str) -> Result<String> {
        debug!(prompt_len = prompt.len(), "Starting LLM generation");
        trace!(prompt = %prompt, "Full prompt");

        let prompt = prompt.to_string();
        let max_tokens = self.max_tokens;
        let temperature = self.temperature;
        let inner = self.inner.clone();

        // Run blocking generation in a separate thread
        let output = tokio::task::spawn_blocking(move || {
            let inner = inner.lock().map_err(|e| {
                ZwobotError::Inference(format!("Failed to acquire lock: {e}"))
            })?;

            let start = std::time::Instant::now();

            // Format as chat message if model has a template
            let formatted_prompt = if let Ok(template) = inner.model.chat_template(None) {
                let messages = vec![LlamaChatMessage::new("user".to_string(), prompt.clone())
                    .map_err(|e| ZwobotError::Inference(format!("Message creation failed: {e}")))?];

                inner
                    .model
                    .apply_chat_template(&template, &messages, true)
                    .map_err(|e| ZwobotError::Inference(format!("Template apply failed: {e}")))?
            } else {
                prompt.clone()
            };

            trace!(formatted = %formatted_prompt, "Formatted prompt");

            // Create context for this generation
            let ctx_params =
                LlamaContextParams::default().with_n_ctx(std::num::NonZeroU32::new(4096));
            let mut ctx = inner
                .model
                .new_context(&inner.backend, ctx_params)
                .map_err(|e| ZwobotError::Inference(format!("Context creation failed: {e}")))?;

            // Tokenize
            let tokens = inner
                .model
                .str_to_token(&formatted_prompt, AddBos::Always)
                .map_err(|e| ZwobotError::Inference(format!("Tokenization failed: {e}")))?;

            debug!(token_count = tokens.len(), "Tokenized prompt");

            // Create batch and add prompt tokens
            let mut batch = LlamaBatch::new(4096, 1);
            for (i, token) in tokens.iter().enumerate() {
                let is_last = i == tokens.len() - 1;
                batch
                    .add(*token, i as i32, &[0], is_last)
                    .map_err(|e| ZwobotError::Inference(format!("Batch add failed: {e}")))?;
            }

            // Process prompt
            ctx.decode(&mut batch)
                .map_err(|e| ZwobotError::Inference(format!("Prompt decode failed: {e}")))?;

            // Set up sampler with temperature
            let mut sampler = if temperature <= 0.0 {
                LlamaSampler::greedy()
            } else {
                LlamaSampler::chain_simple([
                    LlamaSampler::temp(temperature),
                    LlamaSampler::dist(rand::random()),
                ])
            };

            // Generate tokens
            let mut output = String::new();
            // Position in KV cache for next token
            let mut kv_pos = tokens.len() as i32;
            // Batch index where logits are available (last token with logits=true)
            let mut logits_idx = (tokens.len() - 1) as i32;

            for _ in 0..max_tokens {
                let new_token = sampler.sample(&ctx, logits_idx);
                sampler.accept(new_token);

                // Check for end-of-generation
                if inner.model.is_eog_token(new_token) {
                    break;
                }

                // Decode token to string
                if let Ok(token_str) = inner.model.token_to_str(new_token, Special::Tokenize) {
                    output.push_str(&token_str);
                }

                // Prepare next iteration
                batch.clear();
                batch
                    .add(new_token, kv_pos, &[0], true)
                    .map_err(|e| ZwobotError::Inference(format!("Batch add failed: {e}")))?;

                ctx.decode(&mut batch)
                    .map_err(|e| ZwobotError::Inference(format!("Decode failed: {e}")))?;

                kv_pos += 1;
                // After clearing and adding 1 token, logits are at batch index 0
                logits_idx = 0;
            }

            let elapsed = start.elapsed();

            info!(
                elapsed_ms = elapsed.as_millis(),
                output_len = output.len(),
                tokens_generated = kv_pos as usize - tokens.len(),
                "LLM generation complete"
            );
            trace!(output = %output, "Full output");

            Ok::<_, ZwobotError>(output)
        })
        .await
        .map_err(|e| ZwobotError::Inference(format!("Task join error: {e}")))??;

        Ok(output)
    }

    pub fn model_info(&self) -> String {
        let inner = self.inner.lock().ok();
        let (vocab, params) = inner
            .as_ref()
            .map(|i| (i.model.n_vocab(), i.model.n_params()))
            .unwrap_or((0, 0));

        format!(
            "Vocab: {}, Params: {}, Max tokens: {}, Temperature: {}",
            vocab, params, self.max_tokens, self.temperature
        )
    }
}
