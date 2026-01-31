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
///
/// Events are polymorphic - different event types have different fields.
/// We capture the raw data and parse it based on the event type.
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
    /// For submessage events
    #[serde(default)]
    pub submessage: Option<SubMessageEvent>,
    /// Command invocation fields (present when type == "command_invocation")
    #[serde(default)]
    pub command: Option<String>,
    /// Arguments may contain strings, numbers, or booleans from JSON
    #[serde(default)]
    pub arguments: Option<std::collections::HashMap<String, serde_json::Value>>,
    #[serde(default)]
    pub interaction_id: Option<String>,
    #[serde(default)]
    pub message_id: Option<i64>,
    #[serde(default)]
    pub user: Option<User>,
    #[serde(default)]
    pub context: Option<InvocationContext>,
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
    #[serde(default)]
    pub context: serde_json::Value,
    pub user: User,
}

/// Context data for command invocations (stream/topic info)
#[derive(Debug, Clone, Deserialize)]
pub struct InvocationContext {
    pub stream_id: Option<i64>,
    pub topic: Option<String>,
}

/// Raw command invocation event from Tulip
#[derive(Debug, Clone, Deserialize)]
pub struct CommandInvocationEvent {
    pub bot_user_id: i64,
    pub user_profile_id: i64,
    pub message_id: i64,
    pub interaction_id: String,
    pub command: String,
    /// Arguments may contain strings, numbers, or booleans from JSON
    #[serde(default)]
    pub arguments: std::collections::HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub context: Option<InvocationContext>,
    pub user: User,
}

/// Raw submessage event from Tulip
#[derive(Debug, Clone, Deserialize)]
pub struct SubMessageEvent {
    pub message_id: i64,
    pub submessage_id: i64,
    pub sender_id: i64,
    pub msg_type: String,
    #[serde(default)]
    pub content: serde_json::Value,
}

/// A user-owned persona (character identity)
///
/// Personas are personal and portable - they belong to a user and can be
/// used in any channel. Unlike bot-controlled puppets, personas represent
/// the user under a different identity.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Persona {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub bio: Option<String>,
    #[serde(default)]
    pub is_active: bool,
    #[serde(default)]
    pub date_created: Option<i64>,
}

/// A realm persona entry (for @-mention typeahead)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RealmPersona {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    pub user_id: i64,
    pub user_full_name: String,
}

/// Parameters for creating a new persona
#[derive(Debug, Clone, Serialize)]
pub struct CreatePersonaParams {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bio: Option<String>,
}

impl CreatePersonaParams {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            avatar_url: None,
            color: None,
            bio: None,
        }
    }

    pub fn with_avatar(mut self, url: impl Into<String>) -> Self {
        self.avatar_url = Some(url.into());
        self
    }

    pub fn with_color(mut self, color: impl Into<String>) -> Self {
        self.color = Some(color.into());
        self
    }

    pub fn with_bio(mut self, bio: impl Into<String>) -> Self {
        self.bio = Some(bio.into());
        self
    }
}

/// Parameters for updating an existing persona
#[derive(Debug, Clone, Default, Serialize)]
pub struct UpdatePersonaParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bio: Option<String>,
}

impl UpdatePersonaParams {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn avatar_url(mut self, url: impl Into<String>) -> Self {
        self.avatar_url = Some(url.into());
        self
    }

    pub fn color(mut self, color: impl Into<String>) -> Self {
        self.color = Some(color.into());
        self
    }

    pub fn bio(mut self, bio: impl Into<String>) -> Self {
        self.bio = Some(bio.into());
        self
    }
}

/// A channel (stream) in Tulip
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Channel {
    pub stream_id: i64,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub invite_only: bool,
    #[serde(default)]
    pub is_web_public: bool,
    #[serde(default)]
    pub history_public_to_subscribers: bool,
}

/// A topic within a channel
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Topic {
    pub name: String,
    pub max_id: i64,
}

/// A subscription to a channel
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Subscription {
    pub stream_id: i64,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub is_muted: bool,
    #[serde(default)]
    pub pin_to_top: bool,
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
