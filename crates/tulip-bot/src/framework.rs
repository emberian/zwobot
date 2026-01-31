//! Framework for building Tulip bots

use crate::client::TulipClient;
use crate::command::Command;
use crate::context::{Args, AutocompleteContext, CommandContext, CommandInvocationContext, InteractionContext, MessageContext};
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

    /// Sync commands with the Tulip server (register new, update existing, remove stale)
    pub async fn sync_commands(&self) -> Result<()> {
        // Get all commands registered on the server
        let server_commands = self.client.list_commands().await?;
        let bot_id = self.client.bot_id();

        // Filter to just this bot's commands
        let our_commands: std::collections::HashMap<String, i64> = server_commands
            .iter()
            .filter(|c| c.bot_id == bot_id)
            .map(|c| (c.name.clone(), c.id))
            .collect();

        // Get the names of commands we're registering
        let new_command_names: std::collections::HashSet<String> = self
            .commands
            .iter()
            .map(|c| c.definition().name)
            .collect();

        // Unregister commands that are no longer in our list
        for (name, id) in &our_commands {
            if !new_command_names.contains(name) {
                info!("Unregistering stale command /{} (id={})", name, id);
                if let Err(e) = self.client.unregister_command(*id).await {
                    warn!("Failed to unregister command /{}: {}", name, e);
                }
            }
        }

        // Register/update our commands
        for cmd in &self.commands {
            let def = cmd.definition();
            self.client.register_command(&def).await?;
        }

        Ok(())
    }

    /// Run the bot (main event loop)
    pub async fn run(&self) -> Result<()> {
        // Register for message, interaction, command invocation, and submessage events
        let event_types = ["message", "bot_interaction", "command_invocation", "submessage"];
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
                        &autocomplete.context,
                    )
                    .await;
                }
            }
            "command_invocation" => {
                // Command invocation from the UI (slash command)
                if let (Some(command), Some(interaction_id), Some(message_id), Some(user)) = (
                    event.command.as_ref(),
                    event.interaction_id.as_ref(),
                    event.message_id,
                    event.user.as_ref(),
                ) {
                    // Convert JSON values to strings (handles numbers, bools, etc.)
                    let arguments: std::collections::HashMap<String, String> = event
                        .arguments
                        .clone()
                        .unwrap_or_default()
                        .into_iter()
                        .map(|(k, v)| {
                            let s = match v {
                                serde_json::Value::String(s) => s,
                                serde_json::Value::Number(n) => n.to_string(),
                                serde_json::Value::Bool(b) => b.to_string(),
                                other => other.to_string(),
                            };
                            (k, s)
                        })
                        .collect();
                    let (stream_id, topic) = event
                        .context
                        .as_ref()
                        .map(|ctx| (ctx.stream_id, ctx.topic.as_deref()))
                        .unwrap_or((None, None));

                    self.handle_command_invocation(
                        command,
                        arguments,
                        interaction_id,
                        message_id,
                        user,
                        stream_id,
                        topic,
                    )
                    .await;
                }
            }
            "submessage" => {
                // Submessage added to a message (for live-updating widgets like transcripts)
                if let Some(submessage) = event.submessage {
                    self.handle_submessage(submessage).await;
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
        context: &serde_json::Value,
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
                    context,
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

    async fn handle_command_invocation(
        &self,
        command: &str,
        arguments: std::collections::HashMap<String, String>,
        interaction_id: &str,
        message_id: i64,
        user: &crate::types::User,
        stream_id: Option<i64>,
        topic: Option<&str>,
    ) {
        debug!(
            "Command invocation /{} from {} with {} args",
            command,
            user.full_name,
            arguments.len()
        );

        // Find matching command
        for cmd in &self.commands {
            let def = cmd.definition();
            if def.name == command {
                // Convert arguments HashMap to Args
                let args = Args::new(arguments);

                let ctx = CommandInvocationContext::new(
                    command,
                    args,
                    interaction_id,
                    message_id,
                    user,
                    stream_id,
                    topic,
                    &self.client,
                    self.data.as_ref(),
                );

                match cmd.execute_invocation(ctx).await {
                    Ok(response) => {
                        if !response.is_empty() {
                            // Send response to the same location
                            if let (Some(stream_id), Some(topic)) = (stream_id, topic) {
                                if let Err(e) = self
                                    .client
                                    .send_response_to_stream_id(stream_id, topic, &response)
                                    .await
                                {
                                    error!("Failed to send command invocation response: {}", e);
                                }
                            } else {
                                // DM response
                                if let Some(content) = response.content() {
                                    if let Err(e) = self.client.send_private_message(user.id, content).await {
                                        error!("Failed to send private response: {}", e);
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Command invocation {} failed: {}", command, e);
                        // Send error response
                        let error_msg = format!("Error: {}", e);
                        if let (Some(stream_id), Some(topic)) = (stream_id, topic) {
                            let _ = self.client.send_message_to_stream_id(stream_id, topic, &error_msg).await;
                        } else {
                            let _ = self.client.send_private_message(user.id, &error_msg).await;
                        }
                    }
                }
                return;
            }
        }

        warn!("Unknown command invocation: /{}", command);
    }

    async fn handle_submessage(&self, submessage: crate::types::SubMessageEvent) {
        // Log submessage events for debugging
        // Bots can use this to react to transcript entries, etc.
        trace!(
            "Submessage {} on message {} from sender {}: type={}, content={:?}",
            submessage.submessage_id,
            submessage.message_id,
            submessage.sender_id,
            submessage.msg_type,
            submessage.content
        );

        // For now, just log - bots can override behavior by subscribing to submessage events
        // Future: Add submessage handlers similar to interaction handlers
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
/// Supports:
/// - Positional args assigned to options in order
/// - Quoted strings: `"hello world"` or `'hello world'`
/// - If there's only one option, the entire string goes to it
fn parse_args(args_str: &str, options: &[crate::command::CommandOption]) -> Args {
    let mut values = HashMap::new();

    if args_str.is_empty() || options.is_empty() {
        return Args::new(values);
    }

    // If there's only one option, give it the entire string (common case)
    if options.len() == 1 {
        values.insert(options[0].name.clone(), args_str.to_string());
        values.insert("input".to_string(), args_str.to_string());
        return Args::new(values);
    }

    // Parse into tokens respecting quotes
    let tokens = tokenize_args(args_str);

    // Assign tokens to options positionally
    for (i, token) in tokens.iter().enumerate() {
        if i < options.len() {
            values.insert(options[i].name.clone(), token.clone());
        }
    }

    // Also set "input" to the full string for backwards compatibility
    values.insert("input".to_string(), args_str.to_string());

    Args::new(values)
}

/// Tokenize an argument string, respecting quoted strings
fn tokenize_args(s: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quote: Option<char> = None;
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        match (c, in_quote) {
            // Start of quoted string
            ('"' | '\'', None) => {
                in_quote = Some(c);
            }
            // End of quoted string
            (q, Some(quote)) if q == quote => {
                in_quote = None;
                // Don't push empty quoted strings as separate tokens
                if !current.is_empty() || tokens.is_empty() {
                    // Keep the current token - it will be pushed on whitespace or end
                }
            }
            // Whitespace outside quotes = token boundary
            (c, None) if c.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            // Regular character
            (c, _) => {
                current.push(c);
            }
        }
    }

    // Push final token
    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_simple() {
        assert_eq!(tokenize_args("foo bar baz"), vec!["foo", "bar", "baz"]);
    }

    #[test]
    fn test_tokenize_quoted() {
        assert_eq!(
            tokenize_args(r#"foo "hello world" baz"#),
            vec!["foo", "hello world", "baz"]
        );
    }

    #[test]
    fn test_tokenize_single_quotes() {
        assert_eq!(
            tokenize_args("foo 'hello world' baz"),
            vec!["foo", "hello world", "baz"]
        );
    }

    #[test]
    fn test_tokenize_extra_whitespace() {
        assert_eq!(tokenize_args("  foo   bar  "), vec!["foo", "bar"]);
    }

    #[test]
    fn test_tokenize_empty() {
        assert!(tokenize_args("").is_empty());
        assert!(tokenize_args("   ").is_empty());
    }
}
