use crate::config::ZulipConfig;
use crate::error::{Result, ZwobotError};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info, trace};

pub struct ZulipClient {
    client: Client,
    config: ZulipConfig,
    bot_id: i64,
}

#[derive(Debug, Deserialize)]
struct GetMessagesResponse {
    messages: Vec<Message>,
}

#[derive(Debug, Deserialize, Clone)]
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

#[derive(Debug, Deserialize)]
struct UserProfileResponse {
    user_id: i64,
}

#[derive(Debug, Serialize)]
struct SendMessageRequest {
    #[serde(rename = "type")]
    message_type: String,
    to: String,
    topic: String,
    content: String,
}

impl ZulipClient {
    pub async fn new(config: ZulipConfig) -> Result<Self> {
        let client = Client::builder()
            .build()
            .map_err(|e| ZwobotError::ZulipApi(format!("Failed to create HTTP client: {e}")))?;

        let mut zulip_client = Self {
            client,
            config,
            bot_id: 0,
        };

        // Get bot's own user ID
        let bot_id = zulip_client.get_own_user_id().await?;
        zulip_client.bot_id = bot_id;
        info!("Zulip bot initialized with ID: {}", bot_id);

        Ok(zulip_client)
    }

    async fn get_own_user_id(&self) -> Result<i64> {
        let url = format!("{}/api/v1/users/me", self.config.site);
        let response = self
            .client
            .get(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .send()
            .await
            .map_err(|e| ZwobotError::ZulipApi(format!("Failed to get own user: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(ZwobotError::ZulipApi(format!(
                "Failed to get own user ({}): {}",
                status, text
            )));
        }

        let profile: UserProfileResponse = response
            .json()
            .await
            .map_err(|e| ZwobotError::ZulipApi(format!("Failed to parse user profile: {e}")))?;

        Ok(profile.user_id)
    }

    /// Get all messages from a specific channel
    pub async fn get_channel_messages(
        &self,
        channel: &str,
        last_message_id: Option<i64>,
    ) -> Result<Vec<Message>> {
        let (anchor, num_before, num_after) = match last_message_id {
            Some(id) => (id.to_string(), "0", "100"),
            None => ("newest".to_string(), "100", "0"),
        };

        let params = vec![
            ("anchor", anchor),
            ("num_before", num_before.to_string()),
            ("num_after", num_after.to_string()),
            (
                "narrow",
                serde_json::to_string(&vec![
                    serde_json::json!({"operator": "stream", "operand": channel}),
                ])
                .unwrap(),
            ),
        ];

        let url = format!("{}/api/v1/messages", self.config.site);
        let response = self
            .client
            .get(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .query(&params)
            .send()
            .await
            .map_err(|e| ZwobotError::ZulipApi(format!("Failed to fetch messages: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(ZwobotError::ZulipApi(format!(
                "Failed to fetch messages ({}): {}",
                status, text
            )));
        }

        let data: GetMessagesResponse = response
            .json()
            .await
            .map_err(|e| ZwobotError::ZulipApi(format!("Failed to parse messages: {e}")))?;

        Ok(data.messages)
    }

    /// Get all messages from a specific topic in a channel
    pub async fn get_topic_messages(&self, channel: &str, topic: &str) -> Result<Vec<Message>> {
        let narrow = serde_json::to_string(&vec![
            serde_json::json!({"operator": "stream", "operand": channel}),
            serde_json::json!({"operator": "topic", "operand": topic}),
        ])
        .unwrap();

        let url = format!("{}/api/v1/messages", self.config.site);
        let response = self
            .client
            .get(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .query(&[
                ("anchor", "oldest"),
                ("num_before", "0"),
                ("num_after", "1000"),
                ("narrow", &narrow),
            ])
            .send()
            .await
            .map_err(|e| ZwobotError::ZulipApi(format!("Failed to fetch topic messages: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(ZwobotError::ZulipApi(format!(
                "Failed to fetch topic messages ({}): {}",
                status, text
            )));
        }

        let data: GetMessagesResponse = response
            .json()
            .await
            .map_err(|e| ZwobotError::ZulipApi(format!("Failed to parse topic messages: {e}")))?;

        Ok(data.messages)
    }

    /// Send a message to a channel topic
    pub async fn send_message(&self, channel: &str, topic: &str, content: &str) -> Result<()> {
        let url = format!("{}/api/v1/messages", self.config.site);
        let request = SendMessageRequest {
            message_type: "stream".to_string(),
            to: channel.to_string(),
            topic: topic.to_string(),
            content: content.to_string(),
        };

        trace!("Sending message to {}/{}: {}", channel, topic, content);

        let response = self
            .client
            .post(&url)
            .basic_auth(&self.config.email, Some(&self.config.key))
            .form(&request)
            .send()
            .await
            .map_err(|e| ZwobotError::ZulipApi(format!("Failed to send message: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(ZwobotError::ZulipApi(format!(
                "Failed to send message ({}): {}",
                status, text
            )));
        }

        debug!("Message sent successfully to {}/{}", channel, topic);
        Ok(())
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
            .await
            .map_err(|e| ZwobotError::ZulipApi(format!("Failed to add reaction: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(ZwobotError::ZulipApi(format!(
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
            .await
            .map_err(|e| ZwobotError::ZulipApi(format!("Failed to remove reaction: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(ZwobotError::ZulipApi(format!(
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
            .await
            .map_err(|e| ZwobotError::ZulipApi(format!("Failed to register queue: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(ZwobotError::ZulipApi(format!(
                "Failed to register queue ({}): {}",
                status, text
            )));
        }

        #[derive(Deserialize)]
        struct RegisterResponse {
            queue_id: String,
            last_event_id: i64,
        }

        let data: RegisterResponse = response
            .json()
            .await
            .map_err(|e| ZwobotError::ZulipApi(format!("Failed to parse register response: {e}")))?;

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
            .await
            .map_err(|e| ZwobotError::ZulipApi(format!("Failed to get events: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(ZwobotError::ZulipApi(format!(
                "Failed to get events ({}): {}",
                status, text
            )));
        }

        #[derive(Deserialize)]
        struct EventsResponse {
            events: Vec<Event>,
        }

        let data: EventsResponse = response
            .json()
            .await
            .map_err(|e| ZwobotError::ZulipApi(format!("Failed to parse events: {e}")))?;

        Ok(data.events)
    }

    pub fn bot_id(&self) -> i64 {
        self.bot_id
    }
}

#[derive(Debug, Deserialize)]
pub struct Event {
    pub id: i64,
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub message: Option<Message>,
}
