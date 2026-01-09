//! Spweencraft - spween scene execution for Tulip bots
//!
//! This crate enables Tulip bots to execute spween narrative scenes,
//! loading them from designated "registry" channels and running them
//! with live message updates.

pub mod commands;

use smol_str::SmolStr;
use spween::{EffectHandler, Runtime, RuntimeError, Scene, Value};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Errors that can occur during spween operations
#[derive(Debug, Error)]
pub enum SpweenError {
    #[error("Parse error: {0}")]
    Parse(#[from] spween::ParseError),

    #[error("Runtime error: {0}")]
    Runtime(#[from] RuntimeError),

    #[error("Scene not found: {0}")]
    SceneNotFound(String),

    #[error("Invalid choice index: {0}")]
    InvalidChoice(usize),

    #[error("Scene has ended")]
    SceneEnded,

    #[error("Registry channel not found: {0}")]
    RegistryNotFound(String),
}

/// A registry of spween scenes loaded from Tulip channels
#[derive(Debug, Default)]
pub struct SpweenRegistry {
    /// Map from scene ID to parsed scene
    scenes: HashMap<SmolStr, Arc<Scene>>,
    /// Map from channel name to list of scene IDs in that channel
    channel_scenes: HashMap<String, Vec<SmolStr>>,
}

impl SpweenRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a scene from source text
    pub fn register_scene(
        &mut self,
        source: &str,
        channel: &str,
        filename: &str,
    ) -> Result<SmolStr, SpweenError> {
        let scene = spween::parse(source, filename)?;
        let id = scene.meta.id.clone();

        info!("Registered scene '{}' from channel '{}'", id, channel);

        self.scenes.insert(id.clone(), Arc::new(scene));
        self.channel_scenes
            .entry(channel.to_string())
            .or_default()
            .push(id.clone());

        Ok(id)
    }

    /// Get a scene by ID
    pub fn get_scene(&self, id: &str) -> Option<Arc<Scene>> {
        self.scenes.get(id).cloned()
    }

    /// List all scenes in a channel
    pub fn list_channel_scenes(&self, channel: &str) -> Vec<SmolStr> {
        self.channel_scenes
            .get(channel)
            .cloned()
            .unwrap_or_default()
    }

    /// Get all scene IDs
    pub fn all_scene_ids(&self) -> Vec<SmolStr> {
        self.scenes.keys().cloned().collect()
    }

    /// Get scene count
    pub fn scene_count(&self) -> usize {
        self.scenes.len()
    }
}

/// Handler for spween effects that integrates with bot state
#[derive(Debug, Default)]
pub struct BotEffectHandler {
    /// Variables set during scene execution
    pub vars: HashMap<String, Value>,
    /// Items/tags organized by category
    pub has: HashMap<String, Vec<String>>,
    /// Log of function calls made during execution
    pub calls: Vec<(String, Vec<Value>)>,
}

impl BotEffectHandler {
    /// Create a new handler
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with pre-set variables
    pub fn with_vars(vars: HashMap<String, Value>) -> Self {
        Self {
            vars,
            has: HashMap::new(),
            calls: Vec::new(),
        }
    }

    /// Add an item to a category
    pub fn add_item(&mut self, category: impl Into<String>, key: impl Into<String>) {
        self.has
            .entry(category.into())
            .or_default()
            .push(key.into());
    }

    /// Remove an item from a category
    pub fn remove_item(&mut self, category: &str, key: &str) {
        if let Some(items) = self.has.get_mut(category) {
            items.retain(|k| k != key);
        }
    }

    /// Get all function calls made
    pub fn get_calls(&self) -> &[(String, Vec<Value>)] {
        &self.calls
    }

    /// Clear all state
    pub fn clear(&mut self) {
        self.vars.clear();
        self.has.clear();
        self.calls.clear();
    }
}

impl EffectHandler for BotEffectHandler {
    fn get_var(&self, name: &str) -> Value {
        self.vars.get(name).cloned().unwrap_or(Value::Null)
    }

    fn set_var(&mut self, name: &str, value: Value) {
        debug!("Setting var {} = {:?}", name, value);
        self.vars.insert(name.to_string(), value);
    }

    fn has(&self, category: &str, key: &str) -> bool {
        self.has
            .get(category)
            .map(|items| items.iter().any(|k| k == key))
            .unwrap_or(false)
    }

    fn call(&mut self, name: &str, args: &[Value]) -> Result<(), String> {
        debug!("Calling effect: {}({:?})", name, args);
        self.calls.push((name.to_string(), args.to_vec()));
        Ok(())
    }
}

/// State for an active spween execution session
pub struct SpweenSession {
    runtime: Runtime<'static, BotEffectHandler>,
    scene: Arc<Scene>,
}

impl SpweenSession {
    /// Create a new session for a scene
    pub fn new(scene: Arc<Scene>, handler: BotEffectHandler) -> Result<Self, SpweenError> {
        // SAFETY: We keep the scene Arc alive alongside the runtime
        let scene_ref: &'static Scene = unsafe { &*Arc::as_ptr(&scene) };
        let runtime = Runtime::new(scene_ref, handler)?;

        Ok(Self { runtime, scene })
    }

    /// Get the current prose text
    pub fn current_text(&self) -> String {
        self.runtime.current_prose().unwrap_or_default()
    }

    /// Get the available choices with their display text
    pub fn available_choices(&self) -> Vec<(usize, String)> {
        self.runtime
            .available_choices()
            .into_iter()
            .map(|c| (c.index, c.text.to_string()))
            .collect()
    }

    /// Select a choice by index
    pub fn select_choice(&mut self, index: usize) -> Result<(), SpweenError> {
        self.runtime.select_choice(index)?;
        Ok(())
    }

    /// Check if the scene has ended
    pub fn is_ended(&self) -> bool {
        self.runtime.is_ended()
    }

    /// Get the scene metadata
    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// Get a reference to the handler
    pub fn handler(&self) -> &BotEffectHandler {
        self.runtime.handler()
    }

    /// Get a mutable reference to the handler
    pub fn handler_mut(&mut self) -> &mut BotEffectHandler {
        self.runtime.handler_mut()
    }
}

/// Manager for active spween sessions per topic
#[derive(Default)]
pub struct SessionManager {
    /// Active sessions keyed by topic name
    sessions: HashMap<String, SpweenSession>,
}

impl SessionManager {
    /// Create a new session manager
    pub fn new() -> Self {
        Self::default()
    }

    /// Start a new session for a topic
    pub fn start_session(
        &mut self,
        topic: impl Into<String>,
        scene: Arc<Scene>,
        handler: BotEffectHandler,
    ) -> Result<(), SpweenError> {
        let topic = topic.into();
        let session = SpweenSession::new(scene, handler)?;
        self.sessions.insert(topic, session);
        Ok(())
    }

    /// Get a session by topic
    pub fn get_session(&self, topic: &str) -> Option<&SpweenSession> {
        self.sessions.get(topic)
    }

    /// Get a mutable session by topic
    pub fn get_session_mut(&mut self, topic: &str) -> Option<&mut SpweenSession> {
        self.sessions.get_mut(topic)
    }

    /// Remove a session
    pub fn remove_session(&mut self, topic: &str) -> Option<SpweenSession> {
        self.sessions.remove(topic)
    }

    /// Check if a topic has an active session
    pub fn has_session(&self, topic: &str) -> bool {
        self.sessions.contains_key(topic)
    }

    /// Clear all sessions
    pub fn clear(&mut self) {
        self.sessions.clear();
    }
}

/// Shared spweencraft data for the bot framework
pub struct SpweencraftData {
    pub registry: Arc<RwLock<SpweenRegistry>>,
    pub sessions: Arc<RwLock<SessionManager>>,
    pub registry_streams: Vec<String>,
}

impl SpweencraftData {
    /// Create new spweencraft data with registry streams
    pub fn new(registry_streams: Vec<String>) -> Self {
        Self {
            registry: Arc::new(RwLock::new(SpweenRegistry::new())),
            sessions: Arc::new(RwLock::new(SessionManager::new())),
            registry_streams,
        }
    }

    /// Check if a stream is a registry stream
    pub fn is_registry_stream(&self, stream: &str) -> bool {
        self.registry_streams.iter().any(|s| s == stream)
    }

    /// Load a bundled scene from source text
    pub async fn load_bundled_scene(
        &self,
        source: &str,
        filename: &str,
    ) -> Result<SmolStr, SpweenError> {
        let mut registry = self.registry.write().await;
        registry.register_scene(source, "bundled", filename)
    }
}

/// Format a spween display with prose and choices
pub fn format_spween_display(prose: &str, choices: &[(usize, String)]) -> String {
    let mut output = prose.to_string();

    if !choices.is_empty() {
        output.push_str("\n\n---\n\n");
        for (i, text) in choices {
            output.push_str(&format!("**{}**. {}\n", i + 1, text));
        }
    }

    output
}

/// Extract spween scene content from ```spween code blocks
pub fn extract_spween_blocks(content: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut in_spween_block = false;
    let mut current_block = String::new();

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed == "```spween" {
            in_spween_block = true;
            current_block.clear();
        } else if trimmed == "```" && in_spween_block {
            in_spween_block = false;
            if !current_block.is_empty() {
                blocks.push(current_block.clone());
                current_block.clear();
            }
        } else if in_spween_block {
            if !current_block.is_empty() {
                current_block.push('\n');
            }
            current_block.push_str(line);
        }
    }

    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_basic() {
        let mut registry = SpweenRegistry::new();

        let source = r#"---
id: test_scene
title: Test Scene
weight: 10
cooldown: 5
---

=== intro

Hello, world!

* [Continue]
  -> END
"#;

        let id = registry
            .register_scene(source, "test-channel", "test.scene")
            .unwrap();

        assert_eq!(id, "test_scene");
        assert_eq!(registry.scene_count(), 1);
        assert!(registry.get_scene("test_scene").is_some());

        let scenes = registry.list_channel_scenes("test-channel");
        assert_eq!(scenes.len(), 1);
        assert_eq!(scenes[0], "test_scene");
    }

    #[test]
    fn test_bot_handler() {
        let mut handler = BotEffectHandler::new();

        handler.set_var("gold", Value::Int(100));
        assert_eq!(handler.get_var("gold"), Value::Int(100));

        handler.add_item("inventory", "sword");
        assert!(handler.has("inventory", "sword"));
        assert!(!handler.has("inventory", "shield"));

        handler.call("damage", &[Value::Int(10)]).unwrap();
        assert_eq!(handler.get_calls().len(), 1);
    }

    #[test]
    fn test_extract_spween_blocks() {
        let content = r#"Here's a scene:

```spween
---
id: test_scene
title: Test
weight: 10
cooldown: 5
---

=== intro

Hello!

* [Continue]
  -> END
```

And another:

```spween
---
id: another
title: Another
weight: 5
cooldown: 10
---

=== start

Goodbye!
```
"#;

        let blocks = extract_spween_blocks(content);
        assert_eq!(blocks.len(), 2);
        assert!(blocks[0].contains("id: test_scene"));
        assert!(blocks[1].contains("id: another"));
    }

    #[test]
    fn test_session_basic() {
        let source = r#"---
id: test
title: Test
weight: 10
cooldown: 5
---

=== intro

Choose your path.

* [Go left]
  -> END

* [Go right]
  -> END
"#;

        let scene = spween::parse(source, "test.scene").unwrap();
        let handler = BotEffectHandler::new();
        let mut session = SpweenSession::new(Arc::new(scene), handler).unwrap();

        assert!(!session.is_ended());
        assert_eq!(session.current_text(), "Choose your path.");

        let choices = session.available_choices();
        assert_eq!(choices.len(), 2);

        session.select_choice(0).unwrap();
        assert!(session.is_ended());
    }
}
