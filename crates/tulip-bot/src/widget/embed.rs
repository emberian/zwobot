//! Rich embed widget (Discord-style)

use serde::Serialize;

/// A rich embed widget
#[derive(Debug, Clone, Default, Serialize)]
pub struct RichEmbed {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<Author>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<Media>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<Media>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<Field>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footer: Option<Footer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

impl RichEmbed {
    /// Create a new builder for RichEmbed
    pub fn builder() -> RichEmbedBuilder {
        RichEmbedBuilder::default()
    }
}

/// Author information for an embed
#[derive(Debug, Clone, Serialize)]
pub struct Author {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
}

/// Media (image/thumbnail) for an embed
#[derive(Debug, Clone, Serialize)]
pub struct Media {
    pub url: String,
}

/// A field in an embed
#[derive(Debug, Clone, Serialize)]
pub struct Field {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub inline: bool,
}

/// Footer for an embed
#[derive(Debug, Clone, Serialize)]
pub struct Footer {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
}

/// Builder for RichEmbed
#[derive(Debug, Clone, Default)]
pub struct RichEmbedBuilder {
    embed: RichEmbed,
}

impl RichEmbedBuilder {
    /// Set the title
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.embed.title = Some(title.into());
        self
    }

    /// Set the description
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.embed.description = Some(description.into());
        self
    }

    /// Set the URL (makes title clickable)
    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.embed.url = Some(url.into());
        self
    }

    /// Set the color (as RGB integer, e.g., 0x3498db for blue)
    pub fn color(mut self, color: u32) -> Self {
        self.embed.color = Some(color);
        self
    }

    /// Set the author
    pub fn author(mut self, name: impl Into<String>) -> Self {
        self.embed.author = Some(Author {
            name: name.into(),
            url: None,
            icon_url: None,
        });
        self
    }

    /// Set author with URL and icon
    pub fn author_full(
        mut self,
        name: impl Into<String>,
        url: Option<String>,
        icon_url: Option<String>,
    ) -> Self {
        self.embed.author = Some(Author {
            name: name.into(),
            url,
            icon_url,
        });
        self
    }

    /// Set the thumbnail image
    pub fn thumbnail(mut self, url: impl Into<String>) -> Self {
        self.embed.thumbnail = Some(Media { url: url.into() });
        self
    }

    /// Set the main image
    pub fn image(mut self, url: impl Into<String>) -> Self {
        self.embed.image = Some(Media { url: url.into() });
        self
    }

    /// Add a field
    pub fn field(mut self, name: impl Into<String>, value: impl Into<String>, inline: bool) -> Self {
        self.embed.fields.push(Field {
            name: name.into(),
            value: value.into(),
            inline,
        });
        self
    }

    /// Set the footer
    pub fn footer(mut self, text: impl Into<String>) -> Self {
        self.embed.footer = Some(Footer {
            text: text.into(),
            icon_url: None,
        });
        self
    }

    /// Set footer with icon
    pub fn footer_with_icon(mut self, text: impl Into<String>, icon_url: impl Into<String>) -> Self {
        self.embed.footer = Some(Footer {
            text: text.into(),
            icon_url: Some(icon_url.into()),
        });
        self
    }

    /// Set the timestamp (ISO 8601 format)
    pub fn timestamp(mut self, timestamp: impl Into<String>) -> Self {
        self.embed.timestamp = Some(timestamp.into());
        self
    }

    /// Build the embed
    pub fn build(self) -> RichEmbed {
        self.embed
    }
}
