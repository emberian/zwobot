//! Framework for building Tulip bots

use crate::client::TulipClient;
use crate::command::Command;
use crate::context::{Args, AutocompleteContext, CommandContext, InteractionContext, MessageContext};
use crate::error::{Result, TulipError};
use crate::interaction::{Interaction, InteractionHandler};
use crate::response::Response;
use crate::types::{Event, Message, TulipConfig};
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, error, info, trace, warn};

/// Type alias for the boxed message handler function
type BoxedMessageHandler<D> = Box<
    dyn Fn(MessageContext<'_, D>) -> BoxFuture<'_, Result<Option<Response>>> + Send + Sync,
>;

/// The main bot framework
pub struct Framework<D: Send + Sync + 'static> {
    client: Arc<TulipClient>,
    data: Arc<D>,
    commands: Vec<Box<dyn Command<D> + Send + Sync>>,
    interaction_handlers: Vec<Box<dyn InteractionHandler<D> + Send + Sync>>,
    message_handler: Option<BoxedMessageHandler<D>>,
    channel: Option<String>,
}

impl<D: Send + Sync + 'static> Framework<D> {
    /// Create a new framework builder
    pub fn builder() -> FrameworkBuilder<D> {
        FrameworkBuilder::new()
    }

    /// Get the Tulip client
    pub fn client(&self) -> &TulipClient {
        &self.client
    }

    /// Get the shared data
    pub fn data(&self) -> &D {
        &self.data
    }

    /// Register all commands with the Tulip server
    pub async fn register_commands(&self) -> Result<()> {
        for cmd in &self.commands {
            let def = cmd.definition();
            self.client.register_command(&def).await?;
        }
        Ok(())
    }

    /// Run the bot (main event loop)
    pub async fn run(&self) -> Result<()> {
        // Register for message and interaction events
        let event_types = ["message", "bot_interaction"];
        let (mut queue_id, mut last_event_id) = self.client.register_queue(&event_types).await?;

        info!("Framework running, listening for events...");

        loop {
            match self.client.get_events(&queue_id, last_event_id).await {
                Ok(events) => {
                    for event in events {
                        last_event_id = event.id;
                        self.handle_event(event).await;
                    }
                }
                Err(e) => {
                    error!("Error getting events: {}", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

                    // Try to re-register the queue
                    match self.client.register_queue(&event_types).await {
                        Ok((new_queue_id, new_last_event_id)) => {
                            info!("Re-registered event queue");
                            queue_id = new_queue_id;
                            last_event_id = new_last_event_id;
                        }
                        Err(e) => {
                            error!("Failed to re-register queue: {}", e);
                        }
                    }
                }
            }

            // Small delay to avoid hammering the API
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    }

    async fn handle_event(&self, event: Event) {
        match event.event_type.as_str() {
            "message" => {
                if let Some(message) = event.message {
                    self.handle_message(message).await;
                }
            }
            "bot_interaction" => {
                if let Some(interaction_event) = event.interaction {
                    if let Some(interaction) = Interaction::from_event(interaction_event) {
                        self.handle_interaction(interaction).await;
                    }
                }
            }
            "autocomplete" => {
                if let Some(autocomplete) = event.autocomplete {
                    self.handle_autocomplete(
                        &autocomplete.command,
                        &autocomplete.option,
                        &autocomplete.partial,
                        &autocomplete.user,
                    )
                    .await;
                }
            }
            _ => {
                trace!("Ignoring event type: {}", event.event_type);
            }
        }
    }

    async fn handle_message(&self, message: Message) {
        // Ignore messages from the bot itself
        if message.sender_id == self.client.bot_id() {
            return;
        }

        // Get stream name
        let stream_name = match message.stream_name() {
            Some(s) => s,
            None => return, // Not a stream message
        };

        // Check channel filter
        if let Some(ref channel) = self.channel {
            if stream_name != channel {
                return;
            }
        }

        debug!(
            "Received message in {}/{} from {}: {}",
            stream_name,
            message.subject,
            message.sender_full_name,
            message.content.chars().take(50).collect::<String>()
        );

        // Check if it's a command (starts with /)
        let content = message.content.trim();
        if content.starts_with('/') {
            if let Some((cmd_name, args_str)) = parse_command(content) {
                // Find matching command
                for cmd in &self.commands {
                    let def = cmd.definition();
                    if def.name == cmd_name {
                        // Parse arguments
                        let args = parse_args(args_str, &def.options);
                        let ctx = CommandContext::new(
                            &message,
                            &self.client,
                            args,
                            self.data.as_ref(),
                            stream_name,
                        );

                        match cmd.execute(ctx).await {
                            Ok(response) => {
                                if !response.is_empty() {
                                    if let Err(e) = self
                                        .client
                                        .send_response(stream_name, &message.subject, &response)
                                        .await
                                    {
                                        error!("Failed to send command response: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Command {} failed: {}", cmd_name, e);
                                let _ = self
                                    .client
                                    .send_message(
                                        stream_name,
                                        &message.subject,
                                        &format!("Error: {}", e),
                                    )
                                    .await;
                            }
                        }
                        return;
                    }
                }
                // No matching command found
                warn!("Unknown command: /{}", cmd_name);
            }
        }

        // Not a command, try the message handler
        if let Some(ref handler) = self.message_handler {
            let ctx = MessageContext::new(&message, &self.client, self.data.as_ref(), stream_name);
            match handler(ctx).await {
                Ok(Some(response)) => {
                    if !response.is_empty() {
                        if let Err(e) = self
                            .client
                            .send_response(stream_name, &message.subject, &response)
                            .await
                        {
                            error!("Failed to send message response: {}", e);
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    error!("Message handler failed: {}", e);
                }
            }
        }
    }

    async fn handle_interaction(&self, interaction: Interaction) {
        let stream_name = interaction.message.stream_name().unwrap_or("unknown");

        debug!(
            "Received interaction {} from {}",
            interaction.custom_id, interaction.user.full_name
        );

        // Find matching handler
        for handler in &self.interaction_handlers {
            if handler.matches(&interaction.custom_id) {
                let ctx =
                    InteractionContext::new(&interaction, &self.client, self.data.as_ref(), stream_name);

                match handler.handle(ctx).await {
                    Ok(response) => {
                        if !response.is_empty() {
                            if let Err(e) = self
                                .client
                                .send_response(stream_name, &interaction.message.subject, &response)
                                .await
                            {
                                error!("Failed to send interaction response: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Interaction handler failed: {}", e);
                    }
                }
                return;
            }
        }

        warn!("No handler for interaction: {}", interaction.custom_id);
    }

    async fn handle_autocomplete(
        &self,
        command: &str,
        option: &str,
        partial: &str,
        user: &crate::types::User,
    ) {
        debug!(
            "Autocomplete request for /{} option {} partial '{}'",
            command, option, partial
        );

        // Find matching command
        for cmd in &self.commands {
            let def = cmd.definition();
            if def.name == command {
                let ctx = AutocompleteContext::new(
                    command,
                    option,
                    partial,
                    user,
                    &self.client,
                    self.data.as_ref(),
                );

                match cmd.autocomplete(ctx).await {
                    Ok(choices) => {
                        // TODO: Send autocomplete response back to Tulip
                        trace!("Autocomplete returned {} choices", choices.len());
                    }
                    Err(e) => {
                        error!("Autocomplete failed: {}", e);
                    }
                }
                return;
            }
        }
    }
}

/// Builder for Framework
pub struct FrameworkBuilder<D> {
    data: Option<D>,
    commands: Vec<Box<dyn Command<D> + Send + Sync>>,
    interaction_handlers: Vec<Box<dyn InteractionHandler<D> + Send + Sync>>,
    message_handler: Option<BoxedMessageHandler<D>>,
    channel: Option<String>,
}

impl<D: Send + Sync + 'static> FrameworkBuilder<D> {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            data: None,
            commands: Vec::new(),
            interaction_handlers: Vec::new(),
            message_handler: None,
            channel: None,
        }
    }

    /// Set the shared data
    pub fn data(mut self, data: D) -> Self {
        self.data = Some(data);
        self
    }

    /// Add a command
    pub fn command<C: Command<D> + Send + Sync + 'static>(mut self, cmd: C) -> Self {
        self.commands.push(Box::new(cmd));
        self
    }

    /// Add an interaction handler
    pub fn interaction<H: InteractionHandler<D> + Send + Sync + 'static>(mut self, handler: H) -> Self {
        self.interaction_handlers.push(Box::new(handler));
        self
    }

    /// Set the message handler (for non-command messages)
    pub fn on_message<F>(mut self, handler: F) -> Self
    where
        F: for<'a> Fn(MessageContext<'a, D>) -> BoxFuture<'a, Result<Option<Response>>> + Send + Sync + 'static,
    {
        self.message_handler = Some(Box::new(handler));
        self
    }

    /// Filter messages to a specific channel
    pub fn channel(mut self, channel: impl Into<String>) -> Self {
        self.channel = Some(channel.into());
        self
    }

    /// Build the framework
    pub async fn build(self, config: TulipConfig) -> Result<Framework<D>> {
        let data = self
            .data
            .ok_or_else(|| TulipError::Config("No data provided to framework".to_string()))?;

        let client = TulipClient::new(config).await?;

        Ok(Framework {
            client: Arc::new(client),
            data: Arc::new(data),
            commands: self.commands,
            interaction_handlers: self.interaction_handlers,
            message_handler: self.message_handler,
            channel: self.channel,
        })
    }
}

impl<D: Send + Sync + 'static> Default for FrameworkBuilder<D> {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a command from message content
/// Returns (command_name, rest_of_message)
fn parse_command(content: &str) -> Option<(&str, &str)> {
    let content = content.strip_prefix('/')?;
    let mut parts = content.splitn(2, char::is_whitespace);
    let cmd_name = parts.next()?;
    let args = parts.next().unwrap_or("");
    Some((cmd_name, args.trim()))
}

/// Parse arguments from command string
fn parse_args(args_str: &str, _options: &[crate::command::CommandOption]) -> Args {
    // Simple space-separated parsing for now
    // TODO: More sophisticated parsing with quotes, named args, etc.
    let mut values = HashMap::new();

    // For now, treat the entire string as a single positional argument
    // named "input" or the first option name
    if !args_str.is_empty() {
        values.insert("input".to_string(), args_str.to_string());

        // Also set it as the first option if there is one
        if let Some(first_opt) = _options.first() {
            values.insert(first_opt.name.clone(), args_str.to_string());
        }
    }

    Args::new(values)
}
