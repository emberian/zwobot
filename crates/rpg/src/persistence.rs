//! Persistence layer for RPG data

use crate::RpgData;
use std::path::Path;
use thiserror::Error;
use tokio::fs;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Load all persisted data
pub async fn load_all(data: &RpgData) -> Result<(), PersistenceError> {
    // Ensure data directory exists
    fs::create_dir_all(&data.data_dir).await?;

    // Load characters
    let chars_path = data.data_dir.join("characters.json");
    if chars_path.exists() {
        let contents = fs::read_to_string(&chars_path).await?;
        let chars = serde_json::from_str(&contents)?;
        *data.characters.write().await = chars;
        tracing::info!("Loaded characters from {:?}", chars_path);
    }

    // Load combat state
    let combat_path = data.data_dir.join("combat.json");
    if combat_path.exists() {
        let contents = fs::read_to_string(&combat_path).await?;
        let combat = serde_json::from_str(&contents)?;
        *data.combat.write().await = combat;
        tracing::info!("Loaded combat state from {:?}", combat_path);
    }

    Ok(())
}

/// Save all data to disk
pub async fn save_all(data: &RpgData) -> Result<(), PersistenceError> {
    // Ensure data directory exists
    fs::create_dir_all(&data.data_dir).await?;

    // Save characters
    let chars_path = data.data_dir.join("characters.json");
    let chars = data.characters.read().await;
    let json = serde_json::to_string_pretty(&*chars)?;
    write_atomic(&chars_path, json.as_bytes()).await?;

    // Save combat state
    let combat_path = data.data_dir.join("combat.json");
    let combat = data.combat.read().await;
    let json = serde_json::to_string_pretty(&*combat)?;
    write_atomic(&combat_path, json.as_bytes()).await?;

    Ok(())
}

/// Write file atomically (write to temp, then rename)
async fn write_atomic(path: &Path, contents: &[u8]) -> Result<(), std::io::Error> {
    let temp_path = path.with_extension("tmp");
    let mut file = fs::File::create(&temp_path).await?;
    file.write_all(contents).await?;
    file.sync_all().await?;
    fs::rename(&temp_path, path).await?;
    Ok(())
}
