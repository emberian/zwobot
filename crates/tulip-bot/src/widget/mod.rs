//! Widget types for Tulip messages
//!
//! Widgets allow bots to send rich, interactive content.

mod embed;
mod component;
mod freeform;

pub use embed::*;
pub use component::*;
pub use freeform::*;

use serde::Serialize;

/// A widget that can be attached to a message
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "widget_type", content = "extra_data")]
#[serde(rename_all = "snake_case")]
pub enum Widget {
    /// Discord-style rich embed
    RichEmbed(RichEmbed),
    /// Interactive components (buttons, select menus)
    Interactive(Interactive),
    /// Custom HTML/CSS/JS (trusted bots only)
    Freeform(Freeform),
}

impl From<RichEmbed> for Widget {
    fn from(embed: RichEmbed) -> Self {
        Widget::RichEmbed(embed)
    }
}

impl From<Interactive> for Widget {
    fn from(interactive: Interactive) -> Self {
        Widget::Interactive(interactive)
    }
}

impl From<Freeform> for Widget {
    fn from(freeform: Freeform) -> Self {
        Widget::Freeform(freeform)
    }
}
