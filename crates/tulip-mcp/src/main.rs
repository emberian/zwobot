//! Tulip MCP Server
//!
//! An MCP (Model Context Protocol) server that enables AI agents like Claude
//! to interact with Tulip chat servers.
//!
//! # Configuration
//!
//! Set credentials via environment variables:
//! - `TULIP_SITE`: The Tulip server URL (e.g., https://tulip.example.com)
//! - `TULIP_EMAIL`: Bot email address
//! - `TULIP_API_KEY`: Bot API key
//!
//! Or provide a path to a zuliprc file:
//! - `TULIP_ZULIPRC_PATH`: Path to .zuliprc file

use std::env;
use std::sync::Arc;

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler, ServiceExt,
};
use tokio::sync::Mutex;
use tracing::{error, info};
use tulip_bot::prelude::{TulipClient, TulipConfig};

/// Configuration for the Tulip MCP server
#[derive(Debug, Clone)]
struct Config {
    site: String,
    email: String,
    api_key: String,
}

impl Config {
    /// Load configuration from environment variables or zuliprc file
    fn from_env() -> anyhow::Result<Self> {
        // First try direct environment variables
        if let (Ok(site), Ok(email), Ok(api_key)) = (
            env::var("TULIP_SITE"),
            env::var("TULIP_EMAIL"),
            env::var("TULIP_API_KEY"),
        ) {
            return Ok(Self {
                site,
                email,
                api_key,
            });
        }

        // Fall back to zuliprc file
        let zuliprc_path = env::var("TULIP_ZULIPRC_PATH")
            .or_else(|_| env::var("ZULIPRC"))
            .unwrap_or_else(|_| {
                let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
                format!("{}/.zuliprc", home)
            });

        let tulip_config = TulipConfig::from_zuliprc(&zuliprc_path)?;

        Ok(Self {
            site: tulip_config.site,
            email: tulip_config.email,
            api_key: tulip_config.key,
        })
    }

    fn into_tulip_config(self) -> TulipConfig {
        TulipConfig::new(&self.email, &self.api_key, &self.site)
    }
}

// Request types for tools
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct SendMessageRequest {
    #[schemars(description = "The channel (stream) name to send the message to")]
    channel: String,
    #[schemars(description = "The topic within the channel")]
    topic: String,
    #[schemars(description = "The message content (Markdown is supported)")]
    content: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct SendPrivateMessageRequest {
    #[schemars(description = "The user ID to send the private message to")]
    user_id: i64,
    #[schemars(description = "The message content (Markdown is supported)")]
    content: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct ReactionRequest {
    #[schemars(description = "The message ID to add/remove the reaction on")]
    message_id: i64,
    #[schemars(description = "The emoji name (e.g., 'thumbs_up', 'heart', 'tada')")]
    emoji_name: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct GetMessagesRequest {
    #[schemars(description = "The channel (stream) name to get messages from")]
    channel: String,
    #[schemars(description = "The topic to filter by (optional)")]
    topic: Option<String>,
    #[schemars(description = "Number of messages to fetch (default 20, max 100)")]
    limit: Option<i64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct ListTopicsRequest {
    #[schemars(description = "The channel (stream) name to list topics for")]
    channel: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct SubscribeRequest {
    #[schemars(description = "The channel (stream) name to subscribe to")]
    channel: String,
}

/// The MCP handler that provides Tulip tools
#[derive(Clone)]
struct TulipMcpHandler {
    client: Arc<Mutex<TulipClient>>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl TulipMcpHandler {
    fn new(client: TulipClient) -> Self {
        Self {
            client: Arc::new(Mutex::new(client)),
            tool_router: Self::tool_router(),
        }
    }

    /// Send a message to a Tulip channel and topic
    #[tool(description = "Send a message to a Tulip channel (stream) and topic. Returns the message ID on success.")]
    async fn send_message(
        &self,
        Parameters(SendMessageRequest {
            channel,
            topic,
            content,
        }): Parameters<SendMessageRequest>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client.lock().await;
        match client.send_message(&channel, &topic, &content).await {
            Ok(msg_id) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Message sent successfully. Message ID: {}",
                msg_id
            ))])),
            Err(e) => Err(McpError::internal_error(
                format!("Failed to send message: {}", e),
                None,
            )),
        }
    }

    /// Send a private/direct message to a user
    #[tool(description = "Send a private (direct) message to a user by their user ID.")]
    async fn send_private_message(
        &self,
        Parameters(SendPrivateMessageRequest { user_id, content }): Parameters<
            SendPrivateMessageRequest,
        >,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client.lock().await;
        match client.send_private_message(user_id, &content).await {
            Ok(msg_id) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Private message sent. Message ID: {}",
                msg_id
            ))])),
            Err(e) => Err(McpError::internal_error(
                format!("Failed to send private message: {}", e),
                None,
            )),
        }
    }

    /// Add a reaction to a message
    #[tool(description = "Add an emoji reaction to a message. Use emoji names like 'thumbs_up', 'heart', 'tada'.")]
    async fn add_reaction(
        &self,
        Parameters(ReactionRequest {
            message_id,
            emoji_name,
        }): Parameters<ReactionRequest>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client.lock().await;
        match client.add_reaction(message_id, &emoji_name).await {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Reaction '{}' added to message {}",
                emoji_name, message_id
            ))])),
            Err(e) => Err(McpError::internal_error(
                format!("Failed to add reaction: {}", e),
                None,
            )),
        }
    }

    /// Remove a reaction from a message
    #[tool(description = "Remove an emoji reaction from a message.")]
    async fn remove_reaction(
        &self,
        Parameters(ReactionRequest {
            message_id,
            emoji_name,
        }): Parameters<ReactionRequest>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client.lock().await;
        match client.remove_reaction(message_id, &emoji_name).await {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Reaction '{}' removed from message {}",
                emoji_name, message_id
            ))])),
            Err(e) => Err(McpError::internal_error(
                format!("Failed to remove reaction: {}", e),
                None,
            )),
        }
    }

    /// List the bot's personas
    #[tool(description = "List all personas owned by this bot. Personas are alternate identities that can be used to send messages.")]
    async fn list_personas(&self) -> Result<CallToolResult, McpError> {
        let client = self.client.lock().await;
        match client.list_personas().await {
            Ok(personas) => {
                if personas.is_empty() {
                    Ok(CallToolResult::success(vec![Content::text(
                        "No personas found.",
                    )]))
                } else {
                    let list: Vec<String> = personas
                        .iter()
                        .map(|p| format!("- {} (ID: {})", p.name, p.id))
                        .collect();
                    Ok(CallToolResult::success(vec![Content::text(format!(
                        "Personas:\n{}",
                        list.join("\n")
                    ))]))
                }
            }
            Err(e) => Err(McpError::internal_error(
                format!("Failed to list personas: {}", e),
                None,
            )),
        }
    }

    /// Get connection info
    #[tool(description = "Get information about the current Tulip connection, including the bot's user ID.")]
    async fn get_connection_info(&self) -> Result<CallToolResult, McpError> {
        let client = self.client.lock().await;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Connected to Tulip\nBot User ID: {}",
            client.bot_id()
        ))]))
    }

    /// Get messages from a channel
    #[tool(description = "Get recent messages from a Tulip channel. Optionally filter by topic.")]
    async fn get_messages(
        &self,
        Parameters(GetMessagesRequest { channel, topic, limit }): Parameters<GetMessagesRequest>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client.lock().await;
        let num_before = limit.unwrap_or(20).min(100);
        match client.get_messages(&channel, topic.as_deref(), Some(num_before), None, None).await {
            Ok(messages) => {
                if messages.is_empty() {
                    Ok(CallToolResult::success(vec![Content::text("No messages found.")]))
                } else {
                    let formatted: Vec<String> = messages
                        .iter()
                        .map(|m| {
                            format!(
                                "[{}] {}: {}",
                                m.topic(),
                                m.sender_full_name,
                                m.content
                            )
                        })
                        .collect();
                    Ok(CallToolResult::success(vec![Content::text(formatted.join("\n\n"))]))
                }
            }
            Err(e) => Err(McpError::internal_error(
                format!("Failed to get messages: {}", e),
                None,
            )),
        }
    }

    /// List all available channels
    #[tool(description = "List all channels (streams) available on the Tulip server.")]
    async fn list_channels(&self) -> Result<CallToolResult, McpError> {
        let client = self.client.lock().await;
        match client.list_channels().await {
            Ok(channels) => {
                if channels.is_empty() {
                    Ok(CallToolResult::success(vec![Content::text("No channels found.")]))
                } else {
                    let list: Vec<String> = channels
                        .iter()
                        .map(|c| {
                            let public = if c.is_web_public { " [public]" } else { "" };
                            format!("- {} (ID: {}){}", c.name, c.stream_id, public)
                        })
                        .collect();
                    Ok(CallToolResult::success(vec![Content::text(format!(
                        "Channels:\n{}",
                        list.join("\n")
                    ))]))
                }
            }
            Err(e) => Err(McpError::internal_error(
                format!("Failed to list channels: {}", e),
                None,
            )),
        }
    }

    /// List topics in a channel
    #[tool(description = "List all topics in a specific channel.")]
    async fn list_topics(
        &self,
        Parameters(ListTopicsRequest { channel }): Parameters<ListTopicsRequest>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client.lock().await;
        // First get the channel ID
        match client.list_channels().await {
            Ok(channels) => {
                if let Some(ch) = channels.iter().find(|c| c.name == channel) {
                    match client.list_topics(ch.stream_id).await {
                        Ok(topics) => {
                            if topics.is_empty() {
                                Ok(CallToolResult::success(vec![Content::text("No topics found.")]))
                            } else {
                                let list: Vec<String> = topics
                                    .iter()
                                    .map(|t| format!("- {}", t.name))
                                    .collect();
                                Ok(CallToolResult::success(vec![Content::text(format!(
                                    "Topics in #{}:\n{}",
                                    channel,
                                    list.join("\n")
                                ))]))
                            }
                        }
                        Err(e) => Err(McpError::internal_error(
                            format!("Failed to list topics: {}", e),
                            None,
                        )),
                    }
                } else {
                    Err(McpError::internal_error(
                        format!("Channel '{}' not found", channel),
                        None,
                    ))
                }
            }
            Err(e) => Err(McpError::internal_error(
                format!("Failed to find channel: {}", e),
                None,
            )),
        }
    }

    /// Get the bot's subscriptions
    #[tool(description = "List channels the bot is currently subscribed to.")]
    async fn get_subscriptions(&self) -> Result<CallToolResult, McpError> {
        let client = self.client.lock().await;
        match client.get_subscriptions().await {
            Ok(subs) => {
                if subs.is_empty() {
                    Ok(CallToolResult::success(vec![Content::text(
                        "Not subscribed to any channels.",
                    )]))
                } else {
                    let list: Vec<String> = subs
                        .iter()
                        .map(|s| format!("- {} (ID: {})", s.name, s.stream_id))
                        .collect();
                    Ok(CallToolResult::success(vec![Content::text(format!(
                        "Subscriptions:\n{}",
                        list.join("\n")
                    ))]))
                }
            }
            Err(e) => Err(McpError::internal_error(
                format!("Failed to get subscriptions: {}", e),
                None,
            )),
        }
    }

    /// Subscribe to a channel
    #[tool(description = "Subscribe to a channel to receive messages from it.")]
    async fn subscribe(
        &self,
        Parameters(SubscribeRequest { channel }): Parameters<SubscribeRequest>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client.lock().await;
        match client.subscribe(&channel).await {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Subscribed to #{}",
                channel
            ))])),
            Err(e) => Err(McpError::internal_error(
                format!("Failed to subscribe: {}", e),
                None,
            )),
        }
    }

    /// Unsubscribe from a channel
    #[tool(description = "Unsubscribe from a channel to stop receiving messages from it.")]
    async fn unsubscribe(
        &self,
        Parameters(SubscribeRequest { channel }): Parameters<SubscribeRequest>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client.lock().await;
        match client.unsubscribe(&channel).await {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Unsubscribed from #{}",
                channel
            ))])),
            Err(e) => Err(McpError::internal_error(
                format!("Failed to unsubscribe: {}", e),
                None,
            )),
        }
    }
}

#[tool_handler]
impl ServerHandler for TulipMcpHandler {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Tulip MCP Server - Send messages, reactions, and manage personas on Tulip chat. \
                 Use send_message to post to channels, send_private_message for DMs, \
                 and add_reaction to react to messages."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // CRITICAL: Log to stderr only - stdout is for JSON-RPC protocol
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_ansi(false)
        .init();

    info!("Starting Tulip MCP Server");

    // Load configuration
    let config = Config::from_env().map_err(|e| {
        error!("Failed to load configuration: {}", e);
        e
    })?;

    info!("Loaded configuration for {}", config.site);

    // Create Tulip client
    let tulip_config = config.into_tulip_config();
    let client = TulipClient::new(tulip_config).await.map_err(|e| {
        error!("Failed to initialize Tulip client: {}", e);
        e
    })?;

    info!("Connected to Tulip, starting MCP server");

    // Create handler and run with STDIO transport
    let handler = TulipMcpHandler::new(client);
    let service = handler.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;

    Ok(())
}
