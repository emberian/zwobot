//! Core types for Tulip API

use serde::{Deserialize, Serialize};

/// A Zulip/Tulip message
#[derive(Debug, Clone, Deserialize)]
pub struct Message {
    pub id: i64,
    #[serde(rename = "type")]
    pub message_type: String,
    pub content: String,
    pub sender_id: i64,
    pub sender_full_name: String,
    pub sender_email: String,
    pub timestamp: i64,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub display_recipient: serde_json::Value,
}

impl Message {
    /// Get the stream name if this is a stream message
    pub fn stream_name(&self) -> Option<&str> {
        match &self.display_recipient {
            serde_json::Value::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Get the topic (subject) of this message
    pub fn topic(&self) -> &str {
        &self.subject
    }
}

/// A user in the system
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct User {
    pub id: i64,
    pub email: String,
    pub full_name: String,
}

/// An event from the Tulip event queue
#[derive(Debug, Deserialize)]
pub struct Event {
    pub id: i64,
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub message: Option<Message>,
    /// For bot_interaction events
    #[serde(default)]
    pub interaction: Option<InteractionEvent>,
    /// For autocomplete events
    #[serde(default)]
    pub autocomplete: Option<AutocompleteEvent>,
}

/// Raw interaction event from Tulip
#[derive(Debug, Clone, Deserialize)]
pub struct InteractionEvent {
    pub interaction_id: String,
    pub interaction_type: String,
    pub custom_id: String,
    #[serde(default)]
    pub data: serde_json::Value,
    pub message: Message,
    pub user: User,
}

/// Raw autocomplete event from Tulip
#[derive(Debug, Clone, Deserialize)]
pub struct AutocompleteEvent {
    pub command: String,
    pub option: String,
    pub partial: String,
    pub user: User,
}

/// Configuration for connecting to Tulip
#[derive(Debug, Clone)]
pub struct TulipConfig {
    pub email: String,
    pub key: String,
    pub site: String,
}

impl TulipConfig {
    /// Load configuration from a zuliprc file
    pub fn from_zuliprc(path: &str) -> crate::Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| crate::TulipError::Config(format!("Failed to read {}: {}", path, e)))?;

        let mut email = String::new();
        let mut key = String::new();
        let mut site = String::new();

        for line in content.lines() {
            let line = line.trim();
            if let Some(value) = line.strip_prefix("email=") {
                email = value.to_string();
            } else if let Some(value) = line.strip_prefix("key=") {
                key = value.to_string();
            } else if let Some(value) = line.strip_prefix("site=") {
                site = value.to_string();
            }
        }

        if email.is_empty() || key.is_empty() || site.is_empty() {
            return Err(crate::TulipError::Config(
                "Missing email, key, or site in zuliprc".to_string(),
            ));
        }

        Ok(Self { email, key, site })
    }

    /// Create config from individual values
    pub fn new(email: impl Into<String>, key: impl Into<String>, site: impl Into<String>) -> Self {
        Self {
            email: email.into(),
            key: key.into(),
            site: site.into(),
        }
    }
}
