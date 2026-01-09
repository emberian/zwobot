//! Tulip API client

use crate::command::CommandDef;
use crate::error::{Result, TulipError};
use crate::response::Response;
use crate::types::{Event, TulipConfig};
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
}

#[derive(Debug, Deserialize)]
struct ListCommandsResponse {
    commands: Vec<RegisteredCommand>,
}

#[derive(Debug, Deserialize)]
struct UploadFileResponse {
    uri: String,
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
    /// Puppets allow bots to send messages with custom names and avatars.
    /// The stream must have puppet mode enabled.
    pub async fn send_message_as_puppet(
        &self,
        channel: &str,
        topic: &str,
        content: &str,
        puppet_name: &str,
        puppet_avatar_url: Option<&str>,
    ) -> Result<i64> {
        let url = format!("{}/api/v1/messages", self.config.site);

        let mut params = HashMap::new();
        params.insert("type", "stream".to_string());
        params.insert("to", channel.to_string());
        params.insert("topic", topic.to_string());
        params.insert("content", content.to_string());
        params.insert("puppet_name", puppet_name.to_string());
        if let Some(avatar) = puppet_avatar_url {
            params.insert("puppet_avatar_url", avatar.to_string());
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

        let content = response.content().unwrap_or("");

        // TODO: Handle ephemeral and private responses when Tulip API supports them
        // For now, just send as regular messages

        if let Some(widget) = response.widget() {
            let id = self.send_message_with_widget(channel, topic, content, widget).await?;
            Ok(Some(id))
        } else {
            let id = self.send_message(channel, topic, content).await?;
            Ok(Some(id))
        }
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
        let url = format!("{}/api/v1/bot_commands/register", self.config.site);

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
}
