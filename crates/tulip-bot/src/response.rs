//! Response builder for bot replies

use crate::widget::{Interactive, RichEmbed, Widget};

/// A response to send back to Tulip
#[derive(Debug, Clone, Default)]
pub struct Response {
    pub(crate) content: Option<String>,
    pub(crate) widget: Option<Widget>,
    pub(crate) ephemeral: bool,
    pub(crate) visible_user_ids: Option<Vec<i64>>,
}

impl Response {
    /// Create a simple text message
    pub fn message(content: impl Into<String>) -> Self {
        Self {
            content: Some(content.into()),
            ..Default::default()
        }
    }

    /// Create a response with a rich embed
    pub fn embed(embed: RichEmbed) -> Self {
        Self {
            widget: Some(Widget::RichEmbed(embed)),
            ..Default::default()
        }
    }

    /// Create a response with interactive components
    pub fn interactive(interactive: Interactive) -> Self {
        Self {
            widget: Some(Widget::Interactive(interactive)),
            ..Default::default()
        }
    }

    /// Create an ephemeral message (only visible to the interacting user)
    pub fn ephemeral(content: impl Into<String>) -> Self {
        Self {
            content: Some(content.into()),
            ephemeral: true,
            ..Default::default()
        }
    }

    /// Create a private message visible only to specific users
    pub fn private(content: impl Into<String>, user_ids: &[i64]) -> Self {
        Self {
            content: Some(content.into()),
            visible_user_ids: Some(user_ids.to_vec()),
            ..Default::default()
        }
    }

    /// Create an empty response (acknowledge without visible reply)
    pub fn empty() -> Self {
        Self::default()
    }

    /// Add a widget to this response
    pub fn with_widget(mut self, widget: impl Into<Widget>) -> Self {
        self.widget = Some(widget.into());
        self
    }

    /// Add or replace content
    pub fn with_content(mut self, content: impl Into<String>) -> Self {
        self.content = Some(content.into());
        self
    }

    /// Make this response ephemeral
    pub fn make_ephemeral(mut self) -> Self {
        self.ephemeral = true;
        self
    }

    /// Make this response visible only to specific users
    pub fn make_private(mut self, user_ids: &[i64]) -> Self {
        self.visible_user_ids = Some(user_ids.to_vec());
        self
    }

    /// Check if this response has any content to send
    pub fn is_empty(&self) -> bool {
        self.content.is_none() && self.widget.is_none()
    }

    /// Get the content
    pub fn content(&self) -> Option<&str> {
        self.content.as_deref()
    }

    /// Get the widget
    pub fn widget(&self) -> Option<&Widget> {
        self.widget.as_ref()
    }

    /// Check if ephemeral
    pub fn is_ephemeral(&self) -> bool {
        self.ephemeral
    }

    /// Get visible user IDs
    pub fn visible_user_ids(&self) -> Option<&[i64]> {
        self.visible_user_ids.as_deref()
    }
}

// Ergonomic conversions
impl From<&str> for Response {
    fn from(s: &str) -> Self {
        Response::message(s)
    }
}

impl From<String> for Response {
    fn from(s: String) -> Self {
        Response::message(s)
    }
}

impl From<RichEmbed> for Response {
    fn from(embed: RichEmbed) -> Self {
        Response::embed(embed)
    }
}

impl From<Interactive> for Response {
    fn from(interactive: Interactive) -> Self {
        Response::interactive(interactive)
    }
}
