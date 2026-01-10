//! Prelude - convenient re-exports for common usage
//!
//! ```ignore
//! use tulip_bot::prelude::*;
//! ```

pub use crate::client::TulipClient;
pub use crate::command::{Choice, Command, CommandDef, CommandOption, OptionType};
pub use crate::context::{Args, AutocompleteContext, CommandContext, CommandInvocationContext, InteractionContext, MessageContext};
pub use crate::error::{Result, TulipError};
pub use crate::framework::{Framework, FrameworkBuilder};
pub use crate::interaction::{
    on_interaction, Interaction, InteractionData, InteractionHandler, InteractionType, PrefixHandler,
};
pub use crate::response::Response;
pub use crate::types::{
    CreatePersonaParams, Event, Message, Persona, RealmPersona, TulipConfig, UpdatePersonaParams,
    User,
};
pub use crate::widget::{
    ActionRow, Button, ButtonStyle, Component, Freeform, Interactive, Modal, RichEmbed,
    RichEmbedBuilder, SelectMenu, SelectOption, TextInput, TextInputStyle, Widget,
    // Game widgets
    Choice as GameChoice, Dialogue, GameWidget, HistoryEntry, Mood, Speaker, Transcript, TranscriptEntry,
};

// Re-export futures BoxFuture for command implementations
pub use futures::future::BoxFuture;
