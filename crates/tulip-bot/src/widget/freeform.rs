//! Freeform widget (custom HTML/CSS/JS - trusted bots only)

use serde::Serialize;

/// A freeform widget with custom HTML, CSS, and JavaScript
///
/// **Warning**: Only available to trusted bots. Attempting to send
/// a freeform widget from an untrusted bot will result in an error.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Freeform {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub css: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub js: Option<String>,
}

impl Freeform {
    /// Create a new freeform widget
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the HTML content
    pub fn html(mut self, html: impl Into<String>) -> Self {
        self.html = Some(html.into());
        self
    }

    /// Set the CSS styles
    pub fn css(mut self, css: impl Into<String>) -> Self {
        self.css = Some(css.into());
        self
    }

    /// Set the JavaScript code
    ///
    /// The JS receives a `ctx` object with:
    /// - `message_id`: ID of the message containing the widget
    /// - `post_interaction(data)`: Send interaction to bot
    /// - `on(event, selector, handler)`: jQuery-style event binding
    /// - `update_html(html)`: Replace widget HTML content
    ///
    /// And a `container` element for the widget DOM.
    pub fn js(mut self, js: impl Into<String>) -> Self {
        self.js = Some(js.into());
        self
    }
}
