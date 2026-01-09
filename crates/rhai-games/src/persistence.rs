//! RON-based persistence layer for rhai-games
//!
//! Provides file-based storage for game state using RON (Rusty Object Notation).
//! Files are stored in a directory structure: `data/rhai-games/{namespace}/{key}.ron`

use rhai::{Dynamic, Map};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;
use tracing::{debug, warn};

/// RON-based persistence manager
pub struct PersistenceManager {
    base_dir: PathBuf,
}

/// Wrapper for serializing Rhai Dynamic values via serde_json
#[derive(Serialize, Deserialize)]
struct PersistedValue {
    value: serde_json::Value,
}

impl PersistenceManager {
    /// Create a new persistence manager with the given base directory
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    /// Get the file path for a namespace/key pair
    fn file_path(&self, namespace: &str, key: &str) -> PathBuf {
        self.base_dir.join(namespace).join(format!("{}.ron", key))
    }

    /// Ensure the namespace directory exists
    async fn ensure_dir(&self, namespace: &str) -> std::io::Result<PathBuf> {
        let dir = self.base_dir.join(namespace);
        fs::create_dir_all(&dir).await?;
        Ok(dir)
    }

    /// Save a Dynamic value to a RON file
    pub async fn save(&self, namespace: &str, key: &str, value: Dynamic) -> Result<(), String> {
        self.ensure_dir(namespace)
            .await
            .map_err(|e| format!("Failed to create directory: {}", e))?;

        let json_value = dynamic_to_json(&value);
        let wrapped = PersistedValue { value: json_value };

        let ron_string = ron::ser::to_string_pretty(&wrapped, ron::ser::PrettyConfig::default())
            .map_err(|e| format!("Failed to serialize to RON: {}", e))?;

        let path = self.file_path(namespace, key);
        fs::write(&path, ron_string)
            .await
            .map_err(|e| format!("Failed to write file: {}", e))?;

        debug!("Persisted {}/{} to {:?}", namespace, key, path);
        Ok(())
    }

    /// Load a Dynamic value from a RON file
    pub async fn load(&self, namespace: &str, key: &str) -> Result<Dynamic, String> {
        let path = self.file_path(namespace, key);

        let content = fs::read_to_string(&path)
            .await
            .map_err(|e| format!("Failed to read file: {}", e))?;

        let wrapped: PersistedValue =
            ron::from_str(&content).map_err(|e| format!("Failed to parse RON: {}", e))?;

        let dynamic = json_to_dynamic(&wrapped.value);
        debug!("Loaded {}/{} from {:?}", namespace, key, path);
        Ok(dynamic)
    }

    /// List all keys in a namespace
    pub async fn list(&self, namespace: &str) -> Result<Vec<String>, String> {
        let dir = self.base_dir.join(namespace);

        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut entries = fs::read_dir(&dir)
            .await
            .map_err(|e| format!("Failed to read directory: {}", e))?;

        let mut keys = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| format!("Failed to read entry: {}", e))?
        {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "ron") {
                if let Some(stem) = path.file_stem() {
                    keys.push(stem.to_string_lossy().into_owned());
                }
            }
        }

        keys.sort();
        debug!("Listed {} keys in namespace {}", keys.len(), namespace);
        Ok(keys)
    }

    /// Delete a key from a namespace
    pub async fn delete(&self, namespace: &str, key: &str) -> Result<bool, String> {
        let path = self.file_path(namespace, key);

        if !path.exists() {
            return Ok(false);
        }

        fs::remove_file(&path)
            .await
            .map_err(|e| format!("Failed to delete file: {}", e))?;

        debug!("Deleted {}/{}", namespace, key);
        Ok(true)
    }

    /// Check if a key exists in a namespace
    pub async fn exists(&self, namespace: &str, key: &str) -> bool {
        self.file_path(namespace, key).exists()
    }

    /// Get the base directory
    pub fn base_dir(&self) -> &PathBuf {
        &self.base_dir
    }
}

/// Convert a Rhai Dynamic value to a serde_json::Value
pub fn dynamic_to_json(d: &Dynamic) -> serde_json::Value {
    use serde_json::Value;

    if d.is_unit() {
        return Value::Null;
    }
    if let Some(b) = d.clone().try_cast::<bool>() {
        return Value::Bool(b);
    }
    if let Some(i) = d.clone().try_cast::<i64>() {
        return Value::Number(i.into());
    }
    if let Some(f) = d.clone().try_cast::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(f) {
            return Value::Number(n);
        }
        warn!("Could not convert f64 {} to JSON number", f);
        return Value::Null;
    }
    if let Some(s) = d.clone().try_cast::<String>() {
        return Value::String(s);
    }
    if let Some(arr) = d.clone().try_cast::<rhai::Array>() {
        return Value::Array(arr.iter().map(dynamic_to_json).collect());
    }
    if let Some(map) = d.clone().try_cast::<Map>() {
        let obj: serde_json::Map<String, Value> = map
            .iter()
            .map(|(k, v)| (k.to_string(), dynamic_to_json(v)))
            .collect();
        return Value::Object(obj);
    }

    // Fallback: try to get string representation
    warn!("Unknown Dynamic type, converting to string: {:?}", d);
    Value::String(d.to_string())
}

/// Convert a serde_json::Value to a Rhai Dynamic value
pub fn json_to_dynamic(v: &serde_json::Value) -> Dynamic {
    use serde_json::Value;

    match v {
        Value::Null => Dynamic::UNIT,
        Value::Bool(b) => Dynamic::from(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Dynamic::from(i)
            } else if let Some(f) = n.as_f64() {
                Dynamic::from(f)
            } else {
                Dynamic::UNIT
            }
        }
        Value::String(s) => Dynamic::from(s.clone()),
        Value::Array(arr) => {
            let rhai_arr: rhai::Array = arr.iter().map(json_to_dynamic).collect();
            Dynamic::from(rhai_arr)
        }
        Value::Object(obj) => {
            let mut map = Map::new();
            for (k, v) in obj {
                map.insert(k.clone().into(), json_to_dynamic(v));
            }
            Dynamic::from(map)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dynamic_json_roundtrip() {
        // Test primitives
        let d = Dynamic::from(42i64);
        let j = dynamic_to_json(&d);
        let d2 = json_to_dynamic(&j);
        assert_eq!(d2.try_cast::<i64>(), Some(42));

        // Test string
        let d = Dynamic::from("hello".to_string());
        let j = dynamic_to_json(&d);
        let d2 = json_to_dynamic(&j);
        assert_eq!(d2.try_cast::<String>(), Some("hello".to_string()));

        // Test array
        let arr: rhai::Array = vec![Dynamic::from(1i64), Dynamic::from(2i64)];
        let d = Dynamic::from(arr);
        let j = dynamic_to_json(&d);
        let d2 = json_to_dynamic(&j);
        let arr2 = d2.try_cast::<rhai::Array>().unwrap();
        assert_eq!(arr2.len(), 2);

        // Test map
        let mut map = Map::new();
        map.insert("foo".into(), Dynamic::from("bar".to_string()));
        let d = Dynamic::from(map);
        let j = dynamic_to_json(&d);
        let d2 = json_to_dynamic(&j);
        let map2 = d2.try_cast::<Map>().unwrap();
        assert_eq!(
            map2.get("foo").unwrap().clone().try_cast::<String>(),
            Some("bar".to_string())
        );
    }
}
