//! Context types passed to handlers

use crate::client::TulipClient;
use crate::error::{Result, TulipError};
use crate::interaction::Interaction;
use crate::response::Response;
use crate::types::{Message, User};
use std::collections::HashMap;
use std::str::FromStr;

/// Parsed command arguments
#[derive(Debug, Clone, Default)]
pub struct Args {
    values: HashMap<String, String>,
}

impl Args {
    /// Create from a map of values
    pub fn new(values: HashMap<String, String>) -> Self {
        Self { values }
    }

    /// Get a required argument, parsing it to type T
    pub fn get<T: FromStr>(&self, name: &str) -> Result<T> {
        let value = self
            .values
            .get(name)
            .ok_or_else(|| TulipError::MissingArgument(name.to_string()))?;

        value.parse().map_err(|_| TulipError::InvalidArgument {
            name: name.to_string(),
            reason: format!("Failed to parse as {}", std::any::type_name::<T>()),
        })
    }

    /// Get an optional argument
    pub fn get_optional<T: FromStr>(&self, name: &str) -> Result<Option<T>> {
        match self.values.get(name) {
            Some(value) => {
                let parsed = value.parse().map_err(|_| TulipError::InvalidArgument {
                    name: name.to_string(),
                    reason: format!("Failed to parse as {}", std::any::type_name::<T>()),
                })?;
                Ok(Some(parsed))
            }
            None => Ok(None),
        }
    }

    /// Get the raw string value
    pub fn get_raw(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(|s| s.as_str())
    }

    /// Check if an argument exists
    pub fn contains(&self, name: &str) -> bool {
        self.values.contains_key(name)
    }
}

/// Context for message handlers
pub struct MessageContext<'a, D> {
    pub message: &'a Message,
    pub client: &'a TulipClient,
    pub data: &'a D,
    channel: &'a str,
}

impl<'a, D> MessageContext<'a, D> {
    /// Create a new message context
    pub fn new(message: &'a Message, client: &'a TulipClient, data: &'a D, channel: &'a str) -> Self {
        Self {
            message,
            client,
            data,
            channel,
        }
    }

    /// Reply to the message
    pub async fn reply(&self, response: impl Into<Response>) -> Result<Option<i64>> {
        let response = response.into();
        self.client
            .send_response(self.channel, &self.message.subject, &response)
            .await
    }

    /// Add a reaction to the message
    pub async fn react(&self, emoji: &str) -> Result<()> {
        self.client.add_reaction(self.message.id, emoji).await
    }

    /// Remove a reaction from the message
    pub async fn unreact(&self, emoji: &str) -> Result<()> {
        self.client.remove_reaction(self.message.id, emoji).await
    }

    /// Get the channel name
    pub fn channel(&self) -> &str {
        self.channel
    }

    /// Get the topic name
    pub fn topic(&self) -> &str {
        &self.message.subject
    }

    /// Get the message content
    pub fn content(&self) -> &str {
        &self.message.content
    }

    /// Get the sender's name
    pub fn sender_name(&self) -> &str {
        &self.message.sender_full_name
    }

    /// Get the sender's ID
    pub fn sender_id(&self) -> i64 {
        self.message.sender_id
    }
}

/// Context for command handlers
pub struct CommandContext<'a, D> {
    pub message: &'a Message,
    pub client: &'a TulipClient,
    pub args: Args,
    pub data: &'a D,
    channel: &'a str,
}

impl<'a, D> CommandContext<'a, D> {
    /// Create a new command context
    pub fn new(
        message: &'a Message,
        client: &'a TulipClient,
        args: Args,
        data: &'a D,
        channel: &'a str,
    ) -> Self {
        Self {
            message,
            client,
            args,
            data,
            channel,
        }
    }

    /// Reply to the command
    pub async fn reply(&self, response: impl Into<Response>) -> Result<Option<i64>> {
        let response = response.into();
        self.client
            .send_response(self.channel, &self.message.subject, &response)
            .await
    }

    /// Add a reaction to the command message
    pub async fn react(&self, emoji: &str) -> Result<()> {
        self.client.add_reaction(self.message.id, emoji).await
    }

    /// Remove a reaction from the command message
    pub async fn unreact(&self, emoji: &str) -> Result<()> {
        self.client.remove_reaction(self.message.id, emoji).await
    }

    /// Get the channel name
    pub fn channel(&self) -> &str {
        self.channel
    }

    /// Get the topic name
    pub fn topic(&self) -> &str {
        &self.message.subject
    }

    /// Get the sender's name
    pub fn sender_name(&self) -> &str {
        &self.message.sender_full_name
    }

    /// Get the sender's ID
    pub fn sender_id(&self) -> i64 {
        self.message.sender_id
    }
}

/// Context for interaction handlers
pub struct InteractionContext<'a, D> {
    pub interaction: &'a Interaction,
    pub client: &'a TulipClient,
    pub data: &'a D,
    channel: &'a str,
}

impl<'a, D> InteractionContext<'a, D> {
    /// Create a new interaction context
    pub fn new(
        interaction: &'a Interaction,
        client: &'a TulipClient,
        data: &'a D,
        channel: &'a str,
    ) -> Self {
        Self {
            interaction,
            client,
            data,
            channel,
        }
    }

    /// Reply to the interaction
    pub async fn reply(&self, response: impl Into<Response>) -> Result<Option<i64>> {
        let response = response.into();
        self.client
            .send_response(self.channel, &self.interaction.message.subject, &response)
            .await
    }

    /// Get the custom_id of the interaction
    pub fn custom_id(&self) -> &str {
        &self.interaction.custom_id
    }

    /// Get the user who triggered the interaction
    pub fn user(&self) -> &User {
        &self.interaction.user
    }

    /// Get the channel name
    pub fn channel(&self) -> &str {
        self.channel
    }

    /// Get the topic name
    pub fn topic(&self) -> &str {
        &self.interaction.message.subject
    }
}

/// Context for autocomplete handlers
pub struct AutocompleteContext<'a, D> {
    pub command: &'a str,
    pub option: &'a str,
    pub partial: &'a str,
    pub user: &'a User,
    pub context: &'a serde_json::Value,
    pub client: &'a TulipClient,
    pub data: &'a D,
}

impl<'a, D> AutocompleteContext<'a, D> {
    /// Create a new autocomplete context
    pub fn new(
        command: &'a str,
        option: &'a str,
        partial: &'a str,
        user: &'a User,
        context: &'a serde_json::Value,
        client: &'a TulipClient,
        data: &'a D,
    ) -> Self {
        Self {
            command,
            option,
            partial,
            user,
            context,
            client,
            data,
        }
    }
}

/// Context for command invocation handlers (slash commands invoked via UI)
pub struct CommandInvocationContext<'a, D> {
    pub command: &'a str,
    pub args: Args,
    pub interaction_id: &'a str,
    pub message_id: i64,
    pub user: &'a User,
    pub stream_id: Option<i64>,
    pub topic: Option<&'a str>,
    pub client: &'a TulipClient,
    pub data: &'a D,
}

impl<'a, D> CommandInvocationContext<'a, D> {
    /// Create a new command invocation context
    pub fn new(
        command: &'a str,
        args: Args,
        interaction_id: &'a str,
        message_id: i64,
        user: &'a User,
        stream_id: Option<i64>,
        topic: Option<&'a str>,
        client: &'a TulipClient,
        data: &'a D,
    ) -> Self {
        Self {
            command,
            args,
            interaction_id,
            message_id,
            user,
            stream_id,
            topic,
            client,
            data,
        }
    }

    /// Reply to the command invocation
    pub async fn reply(&self, response: impl Into<Response>) -> Result<Option<i64>> {
        let response = response.into();
        if let (Some(stream_id), Some(topic)) = (self.stream_id, self.topic) {
            // For stream messages, we need the stream name - use stream_id for now
            // TODO: Look up stream name from stream_id or add stream name to context
            self.client
                .send_response_to_stream_id(stream_id, topic, &response)
                .await
        } else {
            // For DMs, send to the user
            self.client
                .send_private_message(self.user.id, &response.content.unwrap_or_default())
                .await
                .map(Some)
        }
    }

    /// Get the user who invoked the command
    pub fn invoker(&self) -> &User {
        self.user
    }
}
