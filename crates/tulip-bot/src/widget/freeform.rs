//! Freeform widget (custom HTML/CSS/JS - trusted bots only)

use serde::Serialize;

/// An external dependency (script or stylesheet) to load
#[derive(Debug, Clone, Serialize)]
pub struct Dependency {
    pub url: String,
    #[serde(rename = "type")]
    pub dep_type: DependencyType,
}

/// Type of external dependency
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyType {
    Script,
    Style,
}

impl Dependency {
    /// Create a script dependency
    pub fn script(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            dep_type: DependencyType::Script,
        }
    }

    /// Create a stylesheet dependency
    pub fn style(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            dep_type: DependencyType::Style,
        }
    }
}

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
    /// External dependencies (scripts/styles) to load before JS executes
    /// Dependencies are loaded once and shared across all freeform widgets
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<Vec<Dependency>>,
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

    /// Set external dependencies to load
    ///
    /// Dependencies are loaded once and shared across all freeform widgets.
    /// The JS code will not execute until all dependencies are loaded.
    pub fn dependencies(mut self, deps: Vec<Dependency>) -> Self {
        self.dependencies = Some(deps);
        self
    }

    /// Add a single dependency
    pub fn dependency(mut self, dep: Dependency) -> Self {
        self.dependencies
            .get_or_insert_with(Vec::new)
            .push(dep);
        self
    }
}
