//! Standalone model downloader for zwobot.
//!
//! Usage: cargo run --bin download_model -- <model_id> <quantization>
//! Example: cargo run --bin download_model -- Qwen/Qwen3-1.7B Q4_K_M

use anyhow::Result;
use hf_hub::api::sync::Api;
use std::env;
use std::path::PathBuf;
use tracing::{info, debug, error};

fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("download_model=info".parse()?),
        )
        .init();

    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <model_id> <quantization>", args[0]);
        eprintln!("Example: {} Qwen/Qwen3-1.7B Q4_K_M", args[0]);
        std::process::exit(1);
    }

    let model_id = &args[1];
    let quantization = &args[2];

    info!("Downloading model: {} ({})", model_id, quantization);

    match download_model(model_id, quantization) {
        Ok(path) => {
            info!("✓ Model downloaded successfully!");
            info!("  Path: {}", path.display());
            Ok(())
        }
        Err(e) => {
            error!("✗ Download failed: {}", e);
            Err(e)
        }
    }
}

fn download_model(model_id: &str, quantization: &str) -> Result<PathBuf> {
    let path = PathBuf::from(model_id);

    // If it's already a local file, just verify it exists
    if path.exists() || model_id.ends_with(".gguf") {
        info!("Model appears to be a local file: {}", model_id);
        if path.exists() {
            return Ok(path);
        } else {
            anyhow::bail!("Local file does not exist: {}", model_id);
        }
    }

    // Treat as HuggingFace repo
    if !model_id.contains('/') {
        anyhow::bail!("Model ID must be in format 'org/model' (e.g., Qwen/Qwen3-1.7B)");
    }

    info!("Fetching from HuggingFace: {}", model_id);

    let api = Api::new()?;

    // Check if repo already has -GGUF suffix to avoid duplication
    let (gguf_repo, base_repo) = if model_id.ends_with("-GGUF") {
        (model_id.to_string(), model_id.trim_end_matches("-GGUF").to_string())
    } else {
        (format!("{}-GGUF", model_id), model_id.to_string())
    };

    let quant_lower_underscore = quantization.to_lowercase(); // q8_0
    let quant_lower_hyphen = quant_lower_underscore.replace('_', "-"); // q8-0

    // Common GGUF filename patterns to try
    let raw_model_name = model_id.split('/').last().unwrap_or("model");
    let model_name = raw_model_name.trim_end_matches("-GGUF");
    let model_name_lower = model_name.to_lowercase();
    // Convert decimal points to underscores (e.g., "1.7B" -> "1_7b")
    let model_name_underscore = model_name_lower.replace('.', "_");
    let quant_upper = quantization.to_uppercase().replace('-', "_");

    let filenames = vec![
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
    info!("Trying repo: {}", gguf_repo);
    let repo = api.model(gguf_repo.clone());
    for filename in &filenames {
        debug!("  Trying: {}", filename);
        if let Ok(path) = repo.get(filename) {
            info!("✓ Found: {}", filename);
            return Ok(path);
        }
    }

    // Fall back to base repo (without -GGUF suffix)
    info!("Trying repo: {}", base_repo);
    let repo = api.model(base_repo.clone());
    for filename in &filenames {
        debug!("  Trying: {}", filename);
        if let Ok(path) = repo.get(filename) {
            info!("✓ Found: {}", filename);
            return Ok(path);
        }
    }

    anyhow::bail!(
        "Could not find GGUF file in {} or {}.\nTried patterns: {:?}",
        gguf_repo,
        base_repo,
        filenames
    );
}
