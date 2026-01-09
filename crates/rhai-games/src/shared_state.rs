//! Shared state management for rhai-games
//!
//! Provides two scopes of shared state:
//! - **Global**: Accessible by all scripts
//! - **Namespace**: Accessible by scripts in the same namespace

use rhai::{Dynamic, Map};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

use crate::persistence::PersistenceManager;

/// Global and namespace-scoped shared state
pub struct SharedState {
    /// Global state accessible by all scripts
    global: RwLock<Map>,
    /// Per-namespace state
    namespaces: RwLock<HashMap<String, Map>>,
    /// Persistence manager for saving/loading state
    persistence: Option<Arc<PersistenceManager>>,
    /// Track if global state has been modified (for auto-save)
    global_dirty: RwLock<bool>,
    /// Track which namespaces have been modified
    namespace_dirty: RwLock<HashMap<String, bool>>,
}

impl SharedState {
    /// Create a new shared state manager
    pub fn new(persistence: Option<Arc<PersistenceManager>>) -> Self {
        Self {
            global: RwLock::new(Map::new()),
            namespaces: RwLock::new(HashMap::new()),
            persistence,
            global_dirty: RwLock::new(false),
            namespace_dirty: RwLock::new(HashMap::new()),
        }
    }

    /// Load persisted state from disk
    pub async fn load_persisted(&self) -> Result<(), String> {
        let Some(persistence) = &self.persistence else {
            return Ok(());
        };

        // Load global state
        if persistence.exists("_global", "state").await {
            match persistence.load("_global", "state").await {
                Ok(value) => {
                    if let Some(map) = value.try_cast::<Map>() {
                        *self.global.write().await = map;
                        debug!("Loaded global shared state");
                    }
                }
                Err(e) => {
                    debug!("Could not load global state: {}", e);
                }
            }
        }

        // List and load namespace states
        let namespaces = persistence.list(".").await.unwrap_or_default();
        for ns in namespaces {
            if ns.starts_with('_') {
                continue; // Skip special directories
            }
            if persistence.exists(&ns, "_shared").await {
                match persistence.load(&ns, "_shared").await {
                    Ok(value) => {
                        if let Some(map) = value.try_cast::<Map>() {
                            self.namespaces.write().await.insert(ns.clone(), map);
                            debug!("Loaded namespace {} shared state", ns);
                        }
                    }
                    Err(e) => {
                        debug!("Could not load namespace {} state: {}", ns, e);
                    }
                }
            }
        }

        Ok(())
    }

    /// Save dirty state to disk
    pub async fn save_dirty(&self) -> Result<(), String> {
        let Some(persistence) = &self.persistence else {
            return Ok(());
        };

        // Save global state if dirty
        {
            let mut dirty = self.global_dirty.write().await;
            if *dirty {
                let global = self.global.read().await;
                persistence
                    .save("_global", "state", Dynamic::from(global.clone()))
                    .await?;
                *dirty = false;
                debug!("Saved global shared state");
            }
        }

        // Save dirty namespace states
        {
            let mut dirty_map = self.namespace_dirty.write().await;
            let namespaces = self.namespaces.read().await;

            for (ns, is_dirty) in dirty_map.iter_mut() {
                if *is_dirty {
                    if let Some(map) = namespaces.get(ns) {
                        persistence
                            .save(ns, "_shared", Dynamic::from(map.clone()))
                            .await?;
                        *is_dirty = false;
                        debug!("Saved namespace {} shared state", ns);
                    }
                }
            }
        }

        Ok(())
    }

    // ---- Global state operations ----

    /// Get a value from global state
    pub async fn global_get(&self, key: &str) -> Dynamic {
        self.global
            .read()
            .await
            .get(key)
            .cloned()
            .unwrap_or(Dynamic::UNIT)
    }

    /// Set a value in global state
    pub async fn global_set(&self, key: &str, value: Dynamic) {
        self.global.write().await.insert(key.into(), value);
        *self.global_dirty.write().await = true;
    }

    /// Check if a key exists in global state
    pub async fn global_has(&self, key: &str) -> bool {
        self.global.read().await.contains_key(key)
    }

    /// Get all global state
    pub async fn global_all(&self) -> Map {
        self.global.read().await.clone()
    }

    /// Clear global state
    pub async fn global_clear(&self) {
        self.global.write().await.clear();
        *self.global_dirty.write().await = true;
    }

    // ---- Namespace state operations ----

    /// Get a value from namespace state
    pub async fn namespace_get(&self, namespace: &str, key: &str) -> Dynamic {
        self.namespaces
            .read()
            .await
            .get(namespace)
            .and_then(|map| map.get(key).cloned())
            .unwrap_or(Dynamic::UNIT)
    }

    /// Set a value in namespace state
    pub async fn namespace_set(&self, namespace: &str, key: &str, value: Dynamic) {
        let mut namespaces = self.namespaces.write().await;
        namespaces
            .entry(namespace.to_string())
            .or_insert_with(Map::new)
            .insert(key.into(), value);

        self.namespace_dirty
            .write()
            .await
            .insert(namespace.to_string(), true);
    }

    /// Check if a key exists in namespace state
    pub async fn namespace_has(&self, namespace: &str, key: &str) -> bool {
        self.namespaces
            .read()
            .await
            .get(namespace)
            .map_or(false, |map| map.contains_key(key))
    }

    /// Get all state for a namespace
    pub async fn namespace_all(&self, namespace: &str) -> Map {
        self.namespaces
            .read()
            .await
            .get(namespace)
            .cloned()
            .unwrap_or_default()
    }

    /// Clear namespace state
    pub async fn namespace_clear(&self, namespace: &str) {
        if let Some(map) = self.namespaces.write().await.get_mut(namespace) {
            map.clear();
            self.namespace_dirty
                .write()
                .await
                .insert(namespace.to_string(), true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_global_state() {
        let state = SharedState::new(None);

        // Initially empty
        assert!(!state.global_has("foo").await);
        assert!(state.global_get("foo").await.is_unit());

        // Set and get
        state.global_set("foo", Dynamic::from(42i64)).await;
        assert!(state.global_has("foo").await);
        assert_eq!(state.global_get("foo").await.try_cast::<i64>(), Some(42));

        // Clear
        state.global_clear().await;
        assert!(!state.global_has("foo").await);
    }

    #[tokio::test]
    async fn test_namespace_state() {
        let state = SharedState::new(None);

        // Initially empty
        assert!(!state.namespace_has("ns1", "foo").await);

        // Set and get
        state
            .namespace_set("ns1", "foo", Dynamic::from("bar".to_string()))
            .await;
        assert!(state.namespace_has("ns1", "foo").await);
        assert_eq!(
            state.namespace_get("ns1", "foo").await.try_cast::<String>(),
            Some("bar".to_string())
        );

        // Different namespace is isolated
        assert!(!state.namespace_has("ns2", "foo").await);
    }
}
