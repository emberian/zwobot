//! Tulip API client

use crate::command::CommandDef;
use crate::error::{Result, TulipError};
use crate::response::Response;
use crate::types::{CreatePersonaParams, Event, Persona, RealmPersona, TulipConfig, UpdatePersonaParams};
use crate::widget::Widget;
use reqwest::multipart::{Form, Part};
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info, trace};

/// Client for interacting with the Tulip (Zulip) API
#[derive(Clone)]
pub struct TulipClient {
    client: Client,
    config: TulipConfig,
    bot_id: i64,
}

#[derive(Debug, Deserialize)]
struct UserProfileResponse {
    user_id: i64,
}

#[derive(Debug, Deserialize)]
struct SendMessageResponse {
    id: i64,
}

#[derive(Debug, Deserialize)]
struct RegisterQueueResponse {
    queue_id: String,
    last_event_id: i64,
}

#[derive(Debug, Deserialize)]
struct EventsResponse {
    events: Vec<Event>,
}

#[derive(Debug, Deserialize)]
struct RegisterCommandResponse {
    id: i64,
}

/// A registered command
#[derive(Debug, Clone, Deserialize)]
pub struct RegisteredCommand {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub bot_id: i64,
    #[serde(default)]
    pub bot_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListCommandsResponse {
    commands: Vec<RegisteredCommand>,
}

#[derive(Debug, Deserialize)]
struct UploadFileResponse {
    uri: String,
}

#[derive(Debug, Deserialize)]
struct ListPersonasResponse {
    personas: Vec<Persona>,
}

#[derive(Debug, Deserialize)]
struct PersonaResponse {
    persona: Persona,
}

#[derive(Debug, Deserialize)]
struct RealmPersonasResponse {
    personas: Vec<RealmPersona>,
}

impl TulipClient {
    /// Create a new client and connect to Tulip
    pub async fn new(config: TulipConfig) -> Result<Self> {
        let client = Client::builder()
            .build()
            .map_err(|e| TulipError::Api(format!("Failed to create HTTP client: {e}")))?;

        let mut tulip_client = Self {
            client,
            config,
            bot_id: 0,
        };

        // Get bot's own user ID
        let bot_id = tulip_client.get_own_user_id().await?;
        tulip_client.bot_id = bot_id;
        info!("Tulip bot initialized with ID: {}", bot_id);

        Ok(tulip_client)
    }

    async fn get_own_user_id(&self) -> Result<i64> {
        let url = format!("{}/api/v1/users/me", self.config.site);
        let response = self
            .client
            .get(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(TulipError::Api(format!(
                "Failed to get own user ({}): {}",
                status, text
            )));
        }

        let profile: UserProfileResponse = response.json().await?;
        Ok(profile.user_id)
    }

    /// Get the bot's user ID
    pub fn bot_id(&self) -> i64 {
        self.bot_id
    }

    /// Send a message to a channel topic
    pub async fn send_message(&self, channel: &str, topic: &str, content: &str) -> Result<i64> {
        let url = format!("{}/api/v1/messages", self.config.site);

        let mut params = HashMap::new();
        params.insert("type", "stream".to_string());
        params.insert("to", channel.to_string());
        params.insert("topic", topic.to_string());
        params.insert("content", content.to_string());

        trace!("Sending message to {}/{}: {}", channel, topic, content);

        let response = self
            .client
            .post(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .form(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(TulipError::Api(format!(
                "Failed to send message ({}): {}",
                status, text
            )));
        }

        let resp: SendMessageResponse = response.json().await?;
        debug!("Message sent successfully to {}/{}, id={}", channel, topic, resp.id);
        Ok(resp.id)
    }

    /// Send a message as a puppet (custom persona)
    ///
    /// Puppets allow bots to send messages with custom names, avatars, and colors.
    /// The stream must have puppet mode enabled.
    pub async fn send_message_as_puppet(
        &self,
        channel: &str,
        topic: &str,
        content: &str,
        puppet_name: &str,
        puppet_avatar_url: Option<&str>,
        puppet_color: Option<&str>,
    ) -> Result<i64> {
        let url = format!("{}/api/v1/messages", self.config.site);

        let mut params = HashMap::new();
        params.insert("type", "stream".to_string());
        params.insert("to", channel.to_string());
        params.insert("topic", topic.to_string());
        params.insert("content", content.to_string());
        params.insert("puppet_display_name", puppet_name.to_string());
        if let Some(avatar) = puppet_avatar_url {
            params.insert("puppet_avatar_url", avatar.to_string());
        }
        if let Some(color) = puppet_color {
            params.insert("puppet_color", color.to_string());
        }

        trace!(
            "Sending puppet message to {}/{} as '{}': {}",
            channel,
            topic,
            puppet_name,
            content
        );

        let response = self
            .client
            .post(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .form(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(TulipError::Api(format!(
                "Failed to send puppet message ({}): {}",
                status, text
            )));
        }

        let resp: SendMessageResponse = response.json().await?;
        debug!(
            "Puppet message sent to {}/{} as '{}', id={}",
            channel, topic, puppet_name, resp.id
        );
        Ok(resp.id)
    }

    /// Send a message with a widget
    pub async fn send_message_with_widget(
        &self,
        channel: &str,
        topic: &str,
        content: &str,
        widget: &Widget,
    ) -> Result<i64> {
        let url = format!("{}/api/v1/messages", self.config.site);

        let widget_json = serde_json::to_string(widget)?;

        let mut params = HashMap::new();
        params.insert("type", "stream".to_string());
        params.insert("to", channel.to_string());
        params.insert("topic", topic.to_string());
        params.insert("content", content.to_string());
        params.insert("widget_content", widget_json);

        trace!("Sending message with widget to {}/{}", channel, topic);

        let response = self
            .client
            .post(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .form(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(TulipError::Api(format!(
                "Failed to send message with widget ({}): {}",
                status, text
            )));
        }

        let resp: SendMessageResponse = response.json().await?;
        debug!("Message with widget sent to {}/{}, id={}", channel, topic, resp.id);
        Ok(resp.id)
    }

    /// Send a response (handles widgets, ephemeral, private automatically)
    pub async fn send_response(
        &self,
        channel: &str,
        topic: &str,
        response: &Response,
    ) -> Result<Option<i64>> {
        if response.is_empty() {
            return Ok(None);
        }

        // TODO: Handle ephemeral and private responses when Tulip API supports them
        // For now, just send as regular messages

        if let Some(widget) = response.widget() {
            // Zulip requires non-empty content even for widget messages.
            // Use zero-width space as placeholder if no content provided.
            let content = response.content().unwrap_or("\u{200b}");
            let id = self.send_message_with_widget(channel, topic, content, widget).await?;
            Ok(Some(id))
        } else {
            let content = response.content().unwrap_or("");
            let id = self.send_message(channel, topic, content).await?;
            Ok(Some(id))
        }
    }

    /// Send a response to a stream by ID (used when we only have stream_id, not name)
    pub async fn send_response_to_stream_id(
        &self,
        stream_id: i64,
        topic: &str,
        response: &Response,
    ) -> Result<Option<i64>> {
        if response.is_empty() {
            return Ok(None);
        }

        if let Some(widget) = response.widget() {
            // Zulip requires non-empty content even for widget messages.
            // Use zero-width space as placeholder if no content provided.
            let content = response.content().unwrap_or("\u{200b}");
            let id = self.send_message_to_stream_id_with_widget(stream_id, topic, content, widget).await?;
            Ok(Some(id))
        } else {
            let content = response.content().unwrap_or("");
            let id = self.send_message_to_stream_id(stream_id, topic, content).await?;
            Ok(Some(id))
        }
    }

    /// Send a message with a widget to a stream by ID
    pub async fn send_message_to_stream_id_with_widget(
        &self,
        stream_id: i64,
        topic: &str,
        content: &str,
        widget: &Widget,
    ) -> Result<i64> {
        let url = format!("{}/api/v1/messages", self.config.site);

        let widget_json = serde_json::to_string(widget)?;

        let mut params = HashMap::new();
        params.insert("type", "stream".to_string());
        params.insert("to", stream_id.to_string());
        params.insert("topic", topic.to_string());
        params.insert("content", content.to_string());
        params.insert("widget_content", widget_json);

        trace!("Sending widget message to stream_id={}/{}", stream_id, topic);

        let response = self
            .client
            .post(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .form(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(TulipError::Api(format!(
                "Failed to send message with widget ({}): {}",
                status, text
            )));
        }

        let resp: SendMessageResponse = response.json().await?;
        debug!("Widget message sent to stream_id={}/{}, id={}", stream_id, topic, resp.id);
        Ok(resp.id)
    }

    /// Send a message to a stream by ID
    pub async fn send_message_to_stream_id(
        &self,
        stream_id: i64,
        topic: &str,
        content: &str,
    ) -> Result<i64> {
        let url = format!("{}/api/v1/messages", self.config.site);

        let mut params = HashMap::new();
        params.insert("type", "stream".to_string());
        params.insert("to", stream_id.to_string());
        params.insert("topic", topic.to_string());
        params.insert("content", content.to_string());

        trace!("Sending message to stream_id={}/{}: {}", stream_id, topic, content);

        let response = self
            .client
            .post(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .form(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(TulipError::Api(format!(
                "Failed to send message ({}): {}",
                status, text
            )));
        }

        let resp: SendMessageResponse = response.json().await?;
        debug!("Message sent to stream_id={}/{}, id={}", stream_id, topic, resp.id);
        Ok(resp.id)
    }

    /// Send a private/direct message to a user
    pub async fn send_private_message(&self, user_id: i64, content: &str) -> Result<i64> {
        let url = format!("{}/api/v1/messages", self.config.site);

        let mut params = HashMap::new();
        params.insert("type", "private".to_string());
        params.insert("to", format!("[{}]", user_id));
        params.insert("content", content.to_string());

        trace!("Sending private message to user_id={}: {}", user_id, content);

        let response = self
            .client
            .post(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .form(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(TulipError::Api(format!(
                "Failed to send private message ({}): {}",
                status, text
            )));
        }

        let resp: SendMessageResponse = response.json().await?;
        debug!("Private message sent to user_id={}, id={}", user_id, resp.id);
        Ok(resp.id)
    }

    /// Add a reaction emoji to a message
    pub async fn add_reaction(&self, message_id: i64, emoji_name: &str) -> Result<()> {
        let url = format!(
            "{}/api/v1/messages/{}/reactions",
            self.config.site, message_id
        );

        let mut params = HashMap::new();
        params.insert("emoji_name", emoji_name);

        let response = self
            .client
            .post(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .form(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(TulipError::Api(format!(
                "Failed to add reaction ({}): {}",
                status, text
            )));
        }

        trace!("Added reaction {} to message {}", emoji_name, message_id);
        Ok(())
    }

    /// Remove a reaction emoji from a message
    pub async fn remove_reaction(&self, message_id: i64, emoji_name: &str) -> Result<()> {
        let url = format!(
            "{}/api/v1/messages/{}/reactions",
            self.config.site, message_id
        );

        let mut params = HashMap::new();
        params.insert("emoji_name", emoji_name);

        let response = self
            .client
            .delete(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .query(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(TulipError::Api(format!(
                "Failed to remove reaction ({}): {}",
                status, text
            )));
        }

        trace!("Removed reaction {} from message {}", emoji_name, message_id);
        Ok(())
    }

    /// Get messages from a channel/topic
    ///
    /// # Arguments
    /// * `channel` - Channel (stream) name
    /// * `topic` - Topic name (optional, if None gets all messages from channel)
    /// * `num_before` - Number of messages before anchor (default 100)
    /// * `num_after` - Number of messages after anchor (default 0)
    /// * `anchor` - Message ID anchor or "newest" (default "newest")
    pub async fn get_messages(
        &self,
        channel: &str,
        topic: Option<&str>,
        num_before: Option<i64>,
        num_after: Option<i64>,
        anchor: Option<&str>,
    ) -> Result<Vec<crate::types::Message>> {
        let url = format!("{}/api/v1/messages", self.config.site);

        // Build narrow filter
        let mut narrow = vec![serde_json::json!({"operator": "channel", "operand": channel})];
        if let Some(t) = topic {
            narrow.push(serde_json::json!({"operator": "topic", "operand": t}));
        }

        let narrow_str = serde_json::to_string(&narrow).unwrap();
        let num_before_str = num_before.unwrap_or(100).to_string();
        let num_after_str = num_after.unwrap_or(0).to_string();
        let anchor_str = anchor.unwrap_or("newest");

        let response = self
            .client
            .get(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .query(&[
                ("narrow", narrow_str.as_str()),
                ("num_before", &num_before_str),
                ("num_after", &num_after_str),
                ("anchor", anchor_str),
                ("apply_markdown", "false"),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(TulipError::Api(format!(
                "Failed to get messages ({}): {}",
                status, text
            )));
        }

        #[derive(Deserialize)]
        struct MessagesResponse {
            messages: Vec<crate::types::Message>,
        }

        let data: MessagesResponse = response.json().await?;
        Ok(data.messages)
    }

    /// List all channels (streams) the bot can access
    pub async fn list_channels(&self) -> Result<Vec<crate::types::Channel>> {
        let url = format!("{}/api/v1/streams", self.config.site);

        let response = self
            .client
            .get(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(TulipError::Api(format!(
                "Failed to list channels ({}): {}",
                status, text
            )));
        }

        #[derive(Deserialize)]
        struct StreamsResponse {
            streams: Vec<crate::types::Channel>,
        }

        let data: StreamsResponse = response.json().await?;
        Ok(data.streams)
    }

    /// List topics in a channel
    pub async fn list_topics(&self, stream_id: i64) -> Result<Vec<crate::types::Topic>> {
        let url = format!("{}/api/v1/users/me/{}/topics", self.config.site, stream_id);

        let response = self
            .client
            .get(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(TulipError::Api(format!(
                "Failed to list topics ({}): {}",
                status, text
            )));
        }

        #[derive(Deserialize)]
        struct TopicsResponse {
            topics: Vec<crate::types::Topic>,
        }

        let data: TopicsResponse = response.json().await?;
        Ok(data.topics)
    }

    /// Get the bot's current subscriptions
    pub async fn get_subscriptions(&self) -> Result<Vec<crate::types::Subscription>> {
        let url = format!("{}/api/v1/users/me/subscriptions", self.config.site);

        let response = self
            .client
            .get(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(TulipError::Api(format!(
                "Failed to get subscriptions ({}): {}",
                status, text
            )));
        }

        #[derive(Deserialize)]
        struct SubscriptionsResponse {
            subscriptions: Vec<crate::types::Subscription>,
        }

        let data: SubscriptionsResponse = response.json().await?;
        Ok(data.subscriptions)
    }

    /// Subscribe to a channel
    pub async fn subscribe(&self, channel_name: &str) -> Result<()> {
        let url = format!("{}/api/v1/users/me/subscriptions", self.config.site);

        let subscriptions = serde_json::to_string(&[serde_json::json!({"name": channel_name})])?;

        let response = self
            .client
            .post(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .form(&[("subscriptions", subscriptions)])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(TulipError::Api(format!(
                "Failed to subscribe ({}): {}",
                status, text
            )));
        }

        info!("Subscribed to channel: {}", channel_name);
        Ok(())
    }

    /// Unsubscribe from a channel
    pub async fn unsubscribe(&self, channel_name: &str) -> Result<()> {
        let url = format!("{}/api/v1/users/me/subscriptions", self.config.site);

        let subscriptions = serde_json::to_string(&[channel_name])?;

        let response = self
            .client
            .delete(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .form(&[("subscriptions", subscriptions)])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(TulipError::Api(format!(
                "Failed to unsubscribe ({}): {}",
                status, text
            )));
        }

        info!("Unsubscribed from channel: {}", channel_name);
        Ok(())
    }

    /// Register a queue for real-time events
    pub async fn register_queue(&self, event_types: &[&str]) -> Result<(String, i64)> {
        let url = format!("{}/api/v1/register", self.config.site);

        let mut params = HashMap::new();
        params.insert("event_types", serde_json::to_string(&event_types).unwrap());

        let response = self
            .client
            .post(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .form(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(TulipError::Api(format!(
                "Failed to register queue ({}): {}",
                status, text
            )));
        }

        let data: RegisterQueueResponse = response.json().await?;
        info!("Registered event queue: {}", data.queue_id);
        Ok((data.queue_id, data.last_event_id))
    }

    /// Get events from the queue
    pub async fn get_events(&self, queue_id: &str, last_event_id: i64) -> Result<Vec<Event>> {
        let url = format!("{}/api/v1/events", self.config.site);

        let response = self
            .client
            .get(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .query(&[
                ("queue_id", queue_id),
                ("last_event_id", &last_event_id.to_string()),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(TulipError::Api(format!(
                "Failed to get events ({}): {}",
                status, text
            )));
        }

        let data: EventsResponse = response.json().await?;
        Ok(data.events)
    }

    /// Register a slash command with Tulip
    pub async fn register_command(&self, def: &CommandDef) -> Result<i64> {
        let url = format!("{}/api/v1/bot_commands", self.config.site);

        // Tulip expects form-encoded data with options as a JSON string
        let options_json = serde_json::to_string(&def.options)?;

        let mut params = HashMap::new();
        params.insert("name", def.name.clone());
        params.insert("description", def.description.clone());
        params.insert("options", options_json);

        let response = self
            .client
            .post(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .form(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(TulipError::Api(format!(
                "Failed to register command ({}): {}",
                status, text
            )));
        }

        let data: RegisterCommandResponse = response.json().await?;
        info!("Registered command /{} with id={}", def.name, data.id);
        Ok(data.id)
    }

    /// Unregister a command
    pub async fn unregister_command(&self, command_id: i64) -> Result<()> {
        let url = format!("{}/api/v1/bot_commands/{}", self.config.site, command_id);

        let response = self
            .client
            .delete(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(TulipError::Api(format!(
                "Failed to unregister command ({}): {}",
                status, text
            )));
        }

        info!("Unregistered command id={}", command_id);
        Ok(())
    }

    /// List all registered commands
    pub async fn list_commands(&self) -> Result<Vec<RegisteredCommand>> {
        let url = format!("{}/api/v1/bot_commands", self.config.site);

        let response = self
            .client
            .get(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(TulipError::Api(format!(
                "Failed to list commands ({}): {}",
                status, text
            )));
        }

        let data: ListCommandsResponse = response.json().await?;
        Ok(data.commands)
    }

    /// Upload a file and return the URI that can be used in messages.
    ///
    /// The returned URI can be embedded in message content as a markdown image:
    /// `![alt text](uri)` or linked as `[filename](uri)`
    pub async fn upload_file(&self, file_path: impl AsRef<Path>) -> Result<String> {
        let file_path = file_path.as_ref();
        let url = format!("{}/api/v1/user_uploads", self.config.site);

        // Read file contents
        let file_bytes = tokio::fs::read(file_path)
            .await
            .map_err(|e| TulipError::Api(format!("Failed to read file: {}", e)))?;

        // Get filename
        let filename = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();

        // Detect mime type from extension
        let mime_type = match file_path.extension().and_then(|e| e.to_str()) {
            Some("png") => "image/png",
            Some("jpg") | Some("jpeg") => "image/jpeg",
            Some("gif") => "image/gif",
            Some("webp") => "image/webp",
            Some("pdf") => "application/pdf",
            _ => "application/octet-stream",
        };

        // Build multipart form
        let part = Part::bytes(file_bytes)
            .file_name(filename.clone())
            .mime_str(mime_type)
            .map_err(|e| TulipError::Api(format!("Invalid mime type: {}", e)))?;

        let form = Form::new().part("file", part);

        debug!("Uploading file: {}", filename);

        let response = self
            .client
            .post(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .multipart(form)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(TulipError::Api(format!(
                "Failed to upload file ({}): {}",
                status, text
            )));
        }

        let data: UploadFileResponse = response.json().await?;
        info!("Uploaded file {}, uri: {}", filename, data.uri);

        // Return full URL (uri is relative like /user_uploads/...)
        Ok(format!("{}{}", self.config.site, data.uri))
    }

    /// Add a submessage to an existing message.
    ///
    /// Submessages are used for live-updating widgets like transcripts.
    /// The content is JSON-encoded and stored with the specified msg_type.
    pub async fn add_submessage(
        &self,
        message_id: i64,
        msg_type: &str,
        content: &serde_json::Value,
    ) -> Result<()> {
        let url = format!("{}/json/submessage", self.config.site);

        let mut params = HashMap::new();
        params.insert("message_id", message_id.to_string());
        params.insert("msg_type", msg_type.to_string());
        params.insert("content", serde_json::to_string(content)?);

        trace!("Adding submessage to message {}: {:?}", message_id, content);

        let response = self
            .client
            .post(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .form(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(TulipError::Api(format!(
                "Failed to add submessage ({}): {}",
                status, text
            )));
        }

        debug!("Added submessage to message {}", message_id);
        Ok(())
    }

    /// Add a transcript entry to an existing transcript widget.
    ///
    /// This is a convenience method that wraps add_submessage with the
    /// correct msg_type for transcript entries.
    pub async fn add_transcript_entry(
        &self,
        message_id: i64,
        entry: &crate::widget::TranscriptEntry,
    ) -> Result<()> {
        let content = serde_json::json!({
            "type": "transcript_entry",
            "speaker": entry.speaker,
            "text": entry.text,
            "timestamp": entry.timestamp,
            "avatarUrl": entry.avatar_url,
            "speakerColor": entry.speaker_color,
        });

        self.add_submessage(message_id, "widget", &content).await
    }

    // =========================================================================
    // Persona API
    // =========================================================================

    /// List all active personas for the current user
    pub async fn list_personas(&self) -> Result<Vec<Persona>> {
        let url = format!("{}/json/users/me/personas", self.config.site);

        let response = self
            .client
            .get(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(TulipError::Api(format!(
                "Failed to list personas ({}): {}",
                status, text
            )));
        }

        let data: ListPersonasResponse = response.json().await?;
        debug!("Listed {} personas", data.personas.len());
        Ok(data.personas)
    }

    /// Create a new persona
    pub async fn create_persona(&self, params: &CreatePersonaParams) -> Result<Persona> {
        let url = format!("{}/json/users/me/personas", self.config.site);

        let response = self
            .client
            .post(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .json(params)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(TulipError::Api(format!(
                "Failed to create persona ({}): {}",
                status, text
            )));
        }

        let data: PersonaResponse = response.json().await?;
        info!("Created persona '{}' with id={}", data.persona.name, data.persona.id);
        Ok(data.persona)
    }

    /// Update an existing persona
    pub async fn update_persona(
        &self,
        persona_id: i64,
        params: &UpdatePersonaParams,
    ) -> Result<Persona> {
        let url = format!("{}/json/users/me/personas/{}", self.config.site, persona_id);

        let response = self
            .client
            .patch(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .json(params)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(TulipError::Api(format!(
                "Failed to update persona ({}): {}",
                status, text
            )));
        }

        let data: PersonaResponse = response.json().await?;
        debug!("Updated persona id={}", persona_id);
        Ok(data.persona)
    }

    /// Delete a persona (soft-delete, marks as inactive)
    pub async fn delete_persona(&self, persona_id: i64) -> Result<()> {
        let url = format!("{}/json/users/me/personas/{}", self.config.site, persona_id);

        let response = self
            .client
            .delete(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(TulipError::Api(format!(
                "Failed to delete persona ({}): {}",
                status, text
            )));
        }

        info!("Deleted persona id={}", persona_id);
        Ok(())
    }

    /// Get all active personas in the realm (for @-mention typeahead)
    pub async fn get_realm_personas(&self) -> Result<Vec<RealmPersona>> {
        let url = format!("{}/json/realm/personas", self.config.site);

        let response = self
            .client
            .get(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(TulipError::Api(format!(
                "Failed to get realm personas ({}): {}",
                status, text
            )));
        }

        let data: RealmPersonasResponse = response.json().await?;
        debug!("Got {} realm personas", data.personas.len());
        Ok(data.personas)
    }

    /// Send a message as a persona
    ///
    /// Personas are user-owned character identities that can be used anywhere.
    pub async fn send_message_as_persona(
        &self,
        channel: &str,
        topic: &str,
        content: &str,
        persona_id: i64,
    ) -> Result<i64> {
        let url = format!("{}/api/v1/messages", self.config.site);

        let mut params = HashMap::new();
        params.insert("type", "stream".to_string());
        params.insert("to", channel.to_string());
        params.insert("topic", topic.to_string());
        params.insert("content", content.to_string());
        params.insert("persona_id", persona_id.to_string());

        trace!(
            "Sending message as persona {} to {}/{}: {}",
            persona_id,
            channel,
            topic,
            content
        );

        let response = self
            .client
            .post(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .form(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(TulipError::Api(format!(
                "Failed to send persona message ({}): {}",
                status, text
            )));
        }

        let resp: SendMessageResponse = response.json().await?;
        debug!(
            "Persona message sent to {}/{} as persona {}, id={}",
            channel, topic, persona_id, resp.id
        );
        Ok(resp.id)
    }

    // =========================================================================
    // Whisper API
    // =========================================================================

    /// Send a whisper message (visible only to specified recipients)
    ///
    /// Whispers are channel messages that are only visible to specific users
    /// or groups. The sender always has access to their own whispers.
    pub async fn send_whisper(
        &self,
        channel: &str,
        topic: &str,
        content: &str,
        user_ids: Option<&[i64]>,
        group_ids: Option<&[i64]>,
        puppet_ids: Option<&[i64]>,
    ) -> Result<i64> {
        let url = format!("{}/api/v1/messages", self.config.site);

        let mut params = HashMap::new();
        params.insert("type", "stream".to_string());
        params.insert("to", channel.to_string());
        params.insert("topic", topic.to_string());
        params.insert("content", content.to_string());

        if let Some(ids) = user_ids {
            params.insert("whisper_to_user_ids", serde_json::to_string(ids)?);
        }
        if let Some(ids) = group_ids {
            params.insert("whisper_to_group_ids", serde_json::to_string(ids)?);
        }
        if let Some(ids) = puppet_ids {
            params.insert("whisper_to_puppet_ids", serde_json::to_string(ids)?);
        }

        trace!("Sending whisper to {}/{}", channel, topic);

        let response = self
            .client
            .post(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .form(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(TulipError::Api(format!(
                "Failed to send whisper ({}): {}",
                status, text
            )));
        }

        let resp: SendMessageResponse = response.json().await?;
        debug!("Whisper sent to {}/{}, id={}", channel, topic, resp.id);
        Ok(resp.id)
    }
}
