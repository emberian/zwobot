//! LLM engine wrapper using mistral.rs
//!
//! Provides async-friendly inference using local models via mistral.rs.

use mistralrs::{
    Constraint, DiffusionGenerationParams, DiffusionLoaderType, DiffusionModelBuilder, GgufModelBuilder,
    ImageGenerationResponseFormat, IsqType, Model, NormalRequest, Request, RequestBuilder,
    RequestMessage, ResponseOk, SamplingParams, TextMessageRole, TextMessages, TextModelBuilder,
};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::mpsc::channel;
use tracing::{debug, info, trace};

/// Errors that can occur during LLM operations
#[derive(Debug, Error)]
pub enum LlmError {
    #[error("Model loading error: {0}")]
    ModelLoad(String),

    #[error("Inference error: {0}")]
    Inference(String),
}

impl From<anyhow::Error> for LlmError {
    fn from(e: anyhow::Error) -> Self {
        LlmError::Inference(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, LlmError>;

/// Thread-safe wrapper for mistral.rs inference
pub struct LlmEngine {
    model: Arc<Model>,
    max_tokens: u32,
    temperature: f32,
    model_id: String,
    quantization: String,
}

/// Parse quantization string to ISQ type
fn parse_isq_type(quantization: &str) -> Result<IsqType> {
    let normalized = quantization
        .to_uppercase()
        .replace('-', "_")
        .replace('.', "_");
    match normalized.as_str() {
        "Q4_0" => Ok(IsqType::Q4_0),
        "Q4_1" => Ok(IsqType::Q4_1),
        "Q4_K" | "Q4_K_M" | "Q4_K_S" => Ok(IsqType::Q4K),
        "Q5_0" => Ok(IsqType::Q5_0),
        "Q5_1" => Ok(IsqType::Q5_1),
        "Q5_K" | "Q5_K_M" | "Q5_K_S" => Ok(IsqType::Q5K),
        "Q6_K" => Ok(IsqType::Q6K),
        "Q8_0" => Ok(IsqType::Q8_0),
        "Q8_K" => Ok(IsqType::Q8K),
        "Q2_K" => Ok(IsqType::Q2K),
        "Q3_K" | "Q3_K_M" | "Q3_K_S" => Ok(IsqType::Q3K),
        _ => Err(LlmError::ModelLoad(format!(
            "Unknown quantization type: {}. Supported: Q4_K_M, Q8_0, Q5_K_M, etc.",
            quantization
        ))),
    }
}

impl LlmEngine {
    /// Create a new LLM engine with the specified model.
    ///
    /// # Arguments
    /// * `model_id` - Local path to .gguf file or HuggingFace repo (e.g., "org/model")
    /// * `quantization` - Quantization level (e.g., "Q4_K_M", "Q8_0") - used for ISQ with HF models
    /// * `max_tokens` - Maximum tokens to generate
    /// * `temperature` - Sampling temperature (0.0 = greedy)
    pub async fn new(
        model_id: &str,
        quantization: &str,
        max_tokens: u32,
        temperature: f32,
    ) -> Result<Self> {
        info!("Loading model: {}", model_id);

        let model = if model_id.ends_with(".gguf") || std::path::Path::new(model_id).exists() {
            Self::load_gguf(model_id).await?
        } else {
            Self::load_hf(model_id, quantization).await?
        };

        info!("Model loaded successfully");

        Ok(Self {
            model: Arc::new(model),
            max_tokens,
            temperature,
            model_id: model_id.to_string(),
            quantization: quantization.to_string(),
        })
    }

    /// Load a HuggingFace model with ISQ quantization
    async fn load_hf(model_id: &str, quantization: &str) -> Result<Model> {
        let isq = parse_isq_type(quantization)?;

        TextModelBuilder::new(model_id)
            .with_isq(isq)
            .with_logging()
            .build()
            .await
            .map_err(|e| LlmError::ModelLoad(e.to_string()))
    }

    /// Load a local GGUF model
    async fn load_gguf(path: &str) -> Result<Model> {
        let path = std::path::Path::new(path);
        let filename = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| LlmError::ModelLoad("Invalid GGUF path".to_string()))?;
        let dir = path
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or(".");

        GgufModelBuilder::new(dir, vec![filename])
            .with_logging()
            .build()
            .await
            .map_err(|e| LlmError::ModelLoad(e.to_string()))
    }

    /// Returns the current model ID.
    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Returns the current quantization level.
    pub fn quantization(&self) -> &str {
        &self.quantization
    }

    /// Generate text using chat completion (applies chat template).
    ///
    /// Use this for instruction-tuned models.
    pub async fn generate(&self, prompt: &str) -> Result<String> {
        debug!(prompt_len = prompt.len(), "Starting chat completion");
        trace!(prompt = %prompt, "Full prompt");

        let start = std::time::Instant::now();

        // Build chat request with user message
        let messages = TextMessages::new().add_message(TextMessageRole::User, prompt);

        // Build request with sampling params
        let request = RequestBuilder::from(messages)
            .set_sampler_max_len(self.max_tokens as usize)
            .set_sampler_temperature(self.temperature as f64);

        // Send request
        let response = self
            .model
            .send_chat_request(request)
            .await
            .map_err(|e| LlmError::Inference(e.to_string()))?;

        let output = response
            .choices
            .first()
            .and_then(|c| c.message.content.as_ref())
            .cloned()
            .unwrap_or_default();

        let elapsed = start.elapsed();
        info!(
            elapsed_ms = elapsed.as_millis(),
            output_len = output.len(),
            "Chat completion complete"
        );
        trace!(output = %output, "Full output");

        Ok(output)
    }

    /// Generate text using raw completion (no chat template).
    ///
    /// Use this for base models like OLMo 3.
    pub async fn complete(&self, prompt: &str) -> Result<String> {
        debug!(prompt_len = prompt.len(), "Starting raw completion");
        trace!(prompt = %prompt, "Full prompt");

        let start = std::time::Instant::now();

        let (tx, mut rx) = channel(1);

        let sampling_params = SamplingParams {
            temperature: if self.temperature > 0.0 {
                Some(self.temperature as f64)
            } else {
                None
            },
            top_k: if self.temperature <= 0.0 {
                Some(1)
            } else {
                None
            },
            max_len: Some(self.max_tokens as usize),
            ..SamplingParams::deterministic()
        };

        let request = Request::Normal(Box::new(NormalRequest {
            id: 0,
            messages: RequestMessage::Completion {
                text: prompt.to_string(),
                echo_prompt: false,
                best_of: None,
            },
            sampling_params,
            response: tx,
            return_logprobs: false,
            is_streaming: false,
            constraint: Constraint::None,
            suffix: None,
            tools: None,
            tool_choice: None,
            logits_processors: None,
            return_raw_logits: false,
            web_search_options: None,
            model_id: None,
            truncate_sequence: false,
        }));

        self.model
            .inner()
            .get_sender(None)
            .map_err(|e| LlmError::Inference(e.to_string()))?
            .send(request)
            .await
            .map_err(|e| LlmError::Inference(e.to_string()))?;

        let response = rx
            .recv()
            .await
            .ok_or_else(|| LlmError::Inference("Channel closed".to_string()))?;

        let output = match response.as_result() {
            Ok(ResponseOk::CompletionDone(resp)) => resp
                .choices
                .first()
                .map(|c| c.text.clone())
                .unwrap_or_default(),
            Ok(other) => {
                return Err(LlmError::Inference(format!(
                    "Unexpected response type: {:?}",
                    std::mem::discriminant(&other)
                )))
            }
            Err(e) => return Err(LlmError::Inference(e.to_string())),
        };

        let elapsed = start.elapsed();
        info!(
            elapsed_ms = elapsed.as_millis(),
            output_len = output.len(),
            "Raw completion complete"
        );
        trace!(output = %output, "Full output");

        Ok(output)
    }

    /// Get model info string
    pub fn model_info(&self) -> String {
        format!(
            "Model: {}, Quantization: {}, Max tokens: {}, Temperature: {}",
            self.model_id, self.quantization, self.max_tokens, self.temperature
        )
    }
}

/// Thread-safe wrapper for mistral.rs diffusion model inference
pub struct ImageEngine {
    model: Arc<Model>,
    model_id: String,
    default_width: usize,
    default_height: usize,
}

impl ImageEngine {
    /// Create a new Image engine with the specified FLUX model.
    ///
    /// # Arguments
    /// * `model_id` - HuggingFace repo (e.g., "black-forest-labs/FLUX.1-schnell")
    /// * `offloaded` - If true, use offloaded mode for lower memory (~4GB vs ~33GB)
    /// * `default_width` - Default image width
    /// * `default_height` - Default image height
    pub async fn new(
        model_id: &str,
        offloaded: bool,
        default_width: usize,
        default_height: usize,
    ) -> Result<Self> {
        info!("Loading diffusion model: {} (offloaded: {})", model_id, offloaded);

        let loader_type = if offloaded {
            DiffusionLoaderType::FluxOffloaded
        } else {
            DiffusionLoaderType::Flux
        };

        let model = DiffusionModelBuilder::new(model_id, loader_type)
            .with_logging()
            .build()
            .await
            .map_err(|e| LlmError::ModelLoad(e.to_string()))?;

        info!("Diffusion model loaded successfully");

        Ok(Self {
            model: Arc::new(model),
            model_id: model_id.to_string(),
            default_width,
            default_height,
        })
    }

    /// Returns the current model ID.
    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Generate an image from a text prompt.
    ///
    /// Returns the path to the generated image file.
    pub async fn generate(&self, prompt: &str) -> Result<String> {
        self.generate_with_size(prompt, self.default_width, self.default_height)
            .await
    }

    /// Generate an image from a text prompt with custom dimensions.
    ///
    /// Returns the path to the generated image file.
    pub async fn generate_with_size(
        &self,
        prompt: &str,
        width: usize,
        height: usize,
    ) -> Result<String> {
        debug!(prompt_len = prompt.len(), width, height, "Starting image generation");
        trace!(prompt = %prompt, "Full prompt");

        let start = std::time::Instant::now();

        let params = DiffusionGenerationParams { width, height };

        let response = self
            .model
            .generate_image(prompt, ImageGenerationResponseFormat::Url, params)
            .await
            .map_err(|e| LlmError::Inference(e.to_string()))?;

        let image_path = response
            .data
            .first()
            .and_then(|d| d.url.clone())
            .ok_or_else(|| LlmError::Inference("No image URL in response".to_string()))?;

        let elapsed = start.elapsed();
        info!(
            elapsed_secs = elapsed.as_secs_f32(),
            "Image generation complete"
        );

        Ok(image_path)
    }

    /// Get model info string
    pub fn model_info(&self) -> String {
        format!(
            "Model: {}, Default size: {}x{}",
            self.model_id, self.default_width, self.default_height
        )
    }
}
