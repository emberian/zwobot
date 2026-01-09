//! Script loading and hot reload

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::{GameScript, RhaiEngine, RhaiError, Result};

/// Loads and manages Rhai game scripts
pub struct ScriptLoader {
    /// Directory containing scripts
    scripts_dir: PathBuf,
    /// Loaded scripts by ID
    scripts: RwLock<HashMap<String, Arc<GameScript>>>,
}

impl ScriptLoader {
    /// Create a new script loader
    pub fn new(scripts_dir: impl Into<PathBuf>) -> Self {
        Self {
            scripts_dir: scripts_dir.into(),
            scripts: RwLock::new(HashMap::new()),
        }
    }

    /// Load all scripts from the directory
    pub async fn load_all(&self, engine: &RhaiEngine) -> Result<usize> {
        // Ensure directory exists
        if !self.scripts_dir.exists() {
            info!("Creating scripts directory: {:?}", self.scripts_dir);
            fs::create_dir_all(&self.scripts_dir)?;
            return Ok(0);
        }

        let entries = fs::read_dir(&self.scripts_dir)?;
        let mut count = 0;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "rhai").unwrap_or(false) {
                match self.load_script(engine, &path).await {
                    Ok(script) => {
                        let id = script.id.clone();
                        let mut scripts = self.scripts.write().await;
                        scripts.insert(id.clone(), Arc::new(script));
                        info!("Loaded script: {}", id);
                        count += 1;
                    }
                    Err(e) => {
                        warn!("Failed to load script {:?}: {}", path, e);
                    }
                }
            }
        }

        Ok(count)
    }

    /// Load a single script from a file
    async fn load_script(&self, engine: &RhaiEngine, path: &Path) -> Result<GameScript> {
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| RhaiError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid script filename",
            )))?
            .to_string();

        let source = fs::read_to_string(path)?;
        let modified = fs::metadata(path)?.modified()?;

        // Parse metadata
        let meta = GameScript::parse_meta(&source);

        // Compile script
        let ast = engine.compile(&source, &id)?;

        Ok(GameScript {
            id,
            path: path.to_path_buf(),
            ast: Arc::new(ast),
            meta,
            modified,
        })
    }

    /// Reload all scripts
    pub async fn reload_all(&self, engine: &RhaiEngine) -> Result<usize> {
        let mut scripts = self.scripts.write().await;
        scripts.clear();
        drop(scripts);

        self.load_all(engine).await
    }

    /// Reload a specific script by ID
    pub async fn reload_script(&self, engine: &RhaiEngine, id: &str) -> Result<()> {
        let path = {
            let scripts = self.scripts.read().await;
            scripts
                .get(id)
                .map(|s| s.path.clone())
                .ok_or_else(|| RhaiError::ScriptNotFound(id.to_string()))?
        };

        let script = self.load_script(engine, &path).await?;
        let mut scripts = self.scripts.write().await;
        scripts.insert(id.to_string(), Arc::new(script));
        info!("Reloaded script: {}", id);

        Ok(())
    }

    /// Get a script by ID
    pub async fn get(&self, id: &str) -> Option<Arc<GameScript>> {
        self.scripts.read().await.get(id).cloned()
    }

    /// Get all loaded scripts
    pub async fn all(&self) -> Vec<Arc<GameScript>> {
        self.scripts.read().await.values().cloned().collect()
    }

    /// Check for modified scripts and reload them
    pub async fn check_for_changes(&self, engine: &RhaiEngine) -> Result<Vec<String>> {
        let mut reloaded = Vec::new();
        let script_ids: Vec<(String, PathBuf, SystemTime)> = {
            let scripts = self.scripts.read().await;
            scripts
                .values()
                .map(|s| (s.id.clone(), s.path.clone(), s.modified))
                .collect()
        };

        for (id, path, old_modified) in script_ids {
            if let Ok(metadata) = fs::metadata(&path) {
                if let Ok(new_modified) = metadata.modified() {
                    if new_modified > old_modified {
                        if self.reload_script(engine, &id).await.is_ok() {
                            reloaded.push(id);
                        }
                    }
                }
            }
        }

        Ok(reloaded)
    }
}
