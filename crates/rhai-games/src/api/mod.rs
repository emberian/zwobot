//! Rhai API surface for bot scripts
//!
//! This module defines the functions and types available to Rhai scripts.

use std::collections::HashMap;
use rhai::{Dynamic, Map, Array};
use serde::{Deserialize, Serialize};

/// Execution context passed to script functions
#[derive(Clone, Debug, Default)]
pub struct ExecutionContext {
    /// Current topic name
    pub topic: String,
    /// Channel/stream name
    pub channel: String,
    /// Sender's display name
    pub sender_name: String,
    /// Sender's user ID
    pub sender_id: i64,
    /// Message content
    pub content: String,
    /// Command arguments (for command handlers)
    pub args: HashMap<String, String>,
}

impl ExecutionContext {
    /// Get an argument value
    pub fn arg(&self, name: &str) -> Dynamic {
        self.args
            .get(name)
            .map(|s| Dynamic::from(s.clone()))
            .unwrap_or(Dynamic::UNIT)
    }

    /// Check if an argument exists
    pub fn has_arg(&self, name: &str) -> bool {
        self.args.contains_key(name)
    }
}

/// Response from a script
#[derive(Clone, Debug, Default)]
pub struct ScriptResponse {
    /// Text content
    pub content: Option<String>,
    /// Embed data
    pub embed: Option<EmbedData>,
    /// Reactions to add
    pub reactions_add: Vec<String>,
    /// Reactions to remove
    pub reactions_remove: Vec<String>,
    /// Whether to end the session
    pub end_session: bool,
}

impl ScriptResponse {
    /// Create a text response
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: Some(content.into()),
            ..Default::default()
        }
    }

    /// Create an embed response
    pub fn embed(embed: EmbedData) -> Self {
        Self {
            embed: Some(embed),
            ..Default::default()
        }
    }

    /// Check if response has any content
    pub fn is_empty(&self) -> bool {
        self.content.is_none() && self.embed.is_none()
    }
}

/// Embed data for rich responses
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EmbedData {
    pub title: Option<String>,
    pub description: Option<String>,
    pub color: Option<i64>,
    pub fields: Vec<EmbedField>,
    pub footer: Option<String>,
}

/// A field in an embed
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EmbedField {
    pub name: String,
    pub value: String,
    pub inline: bool,
}

impl EmbedData {
    /// Create from a Rhai map
    pub fn from_map(map: &Map) -> Self {
        let mut embed = Self::default();

        if let Some(title) = map.get("title") {
            embed.title = title.clone().try_cast::<String>();
        }
        if let Some(desc) = map.get("description") {
            embed.description = desc.clone().try_cast::<String>();
        }
        if let Some(color) = map.get("color") {
            embed.color = color.clone().try_cast::<i64>();
        }
        if let Some(footer) = map.get("footer") {
            embed.footer = footer.clone().try_cast::<String>();
        }
        if let Some(fields) = map.get("fields") {
            if let Some(fields_arr) = fields.clone().try_cast::<Array>() {
                embed.fields = fields_arr
                    .iter()
                    .filter_map(|f| {
                        let field_map = f.clone().try_cast::<Map>()?;
                        let name = field_map.get("name")?.clone().try_cast::<String>()?;
                        let value = field_map.get("value")?.clone().try_cast::<String>()?;
                        let inline = field_map
                            .get("inline")
                            .and_then(|v| v.clone().try_cast::<bool>())
                            .unwrap_or(false);
                        Some(EmbedField { name, value, inline })
                    })
                    .collect();
            }
        }

        embed
    }
}

/// Response builder used within scripts
///
/// This is stored in thread-local storage and accumulated during script execution.
#[derive(Clone, Debug, Default)]
pub struct ResponseBuilder {
    pub response: ScriptResponse,
}

impl ResponseBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_content(&mut self, content: String) {
        self.response.content = Some(content);
    }

    pub fn set_embed(&mut self, embed: EmbedData) {
        self.response.embed = Some(embed);
    }

    pub fn add_reaction(&mut self, emoji: String) {
        self.response.reactions_add.push(emoji);
    }

    pub fn remove_reaction(&mut self, emoji: String) {
        self.response.reactions_remove.push(emoji);
    }

    pub fn set_end_session(&mut self) {
        self.response.end_session = true;
    }

    pub fn take(self) -> ScriptResponse {
        self.response
    }
}
