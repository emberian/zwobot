//! tulip-bot: A bot framework for Tulip (Zulip fork)
//!
//! This crate provides a poise-like framework for building bots that interact with
//! Tulip's advanced bot features including widgets, slash commands, and interactions.
//!
//! # Features
//!
//! - **Widgets**: Rich embeds, interactive components (buttons, select menus, modals)
//! - **Commands**: Slash command registration with options and autocomplete
//! - **Interactions**: Handle button clicks, select menu selections, modal submissions
//! - **Responses**: Ephemeral, private, or public messages with widgets
//!
//! # Example
//!
//! ```ignore
//! use tulip_bot::prelude::*;
//!
//! struct MyData {
//!     // Your shared state
//! }
//!
//! struct PingCommand;
//!
//! impl Command<MyData> for PingCommand {
//!     fn definition(&self) -> CommandDef {
//!         CommandDef::new("ping", "Respond with pong")
//!     }
//!
//!     fn execute<'a>(&'a self, ctx: CommandContext<'a, MyData>) -> BoxFuture<'a, Result<Response>> {
//!         Box::pin(async move {
//!             Ok(Response::message("Pong!"))
//!         })
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let config = TulipConfig::from_zuliprc("zuliprc")?;
//!
//!     let framework = Framework::builder()
//!         .data(MyData {})
//!         .command(PingCommand)
//!         .build(config)
//!         .await?;
//!
//!     framework.register_commands().await?;
//!     framework.run().await
//! }
//! ```

// Always available - widget types and serialization
pub mod widget;
pub mod error;
pub mod types;

// Runtime-dependent modules
#[cfg(feature = "runtime")]
pub mod client;
#[cfg(feature = "runtime")]
pub mod command;
#[cfg(feature = "runtime")]
pub mod context;
#[cfg(feature = "runtime")]
pub mod framework;
#[cfg(feature = "runtime")]
pub mod interaction;
#[cfg(feature = "runtime")]
pub mod prelude;
#[cfg(feature = "runtime")]
pub mod response;

// Re-export key types at crate root
pub use error::{Result, TulipError};
#[cfg(feature = "runtime")]
pub use types::TulipConfig;
