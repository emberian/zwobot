//! High-level game widgets that use the TulipWidgets runtime
//!
//! These widgets convert to freeform widgets with the game-widgets
//! runtime loaded as a dependency.

use serde::Serialize;
use super::{Freeform, Dependency, Widget};

/// Base URL for the game widgets runtime (hosted on GitHub Pages)
/// Update this when deploying to a different location
pub const GAME_WIDGETS_BASE_URL: &str = "https://ember91.github.io/zwobot/game-widgets";

/// Get the default dependencies for game widgets
pub fn game_widget_dependencies() -> Vec<Dependency> {
    vec![
        Dependency::script(format!("{}/tulip-game-widgets.js", GAME_WIDGETS_BASE_URL)),
        Dependency::style(format!("{}/style.css", GAME_WIDGETS_BASE_URL)),
    ]
}

/// Speaker information for dialogue
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Speaker {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub portrait_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mood: Option<Mood>,
}

/// Speaker mood affects portrait display
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mood {
    Neutral,
    Happy,
    Angry,
    Sad,
    Surprised,
}

/// A choice in a dialogue
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    pub label: String,
    pub custom_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requirement_text: Option<String>,
}

impl Choice {
    pub fn new(label: impl Into<String>, custom_id: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            custom_id: custom_id.into(),
            disabled: None,
            requirement_text: None,
        }
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = Some(true);
        self
    }

    pub fn requirement(mut self, text: impl Into<String>) -> Self {
        self.requirement_text = Some(text.into());
        self
    }
}

/// A dialogue widget for conversations
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dialogue {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker: Option<Speaker>,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<Choice>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continue_custom_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<Vec<HistoryEntry>>,
}

/// A history entry in dialogue
#[derive(Debug, Clone, Serialize)]
pub struct HistoryEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    pub text: String,
}

impl Dialogue {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            speaker: None,
            text: text.into(),
            choices: None,
            continue_custom_id: None,
            history: None,
        }
    }

    pub fn speaker(mut self, name: impl Into<String>) -> Self {
        self.speaker = Some(Speaker {
            name: name.into(),
            portrait_url: None,
            mood: None,
        });
        self
    }

    pub fn speaker_with_portrait(
        mut self,
        name: impl Into<String>,
        portrait_url: impl Into<String>,
    ) -> Self {
        self.speaker = Some(Speaker {
            name: name.into(),
            portrait_url: Some(portrait_url.into()),
            mood: None,
        });
        self
    }

    pub fn mood(mut self, mood: Mood) -> Self {
        if let Some(ref mut speaker) = self.speaker {
            speaker.mood = Some(mood);
        }
        self
    }

    pub fn choices(mut self, choices: Vec<Choice>) -> Self {
        self.choices = Some(choices);
        self
    }

    pub fn choice(mut self, label: impl Into<String>, custom_id: impl Into<String>) -> Self {
        self.choices
            .get_or_insert_with(Vec::new)
            .push(Choice::new(label, custom_id));
        self
    }

    pub fn continue_button(mut self, custom_id: impl Into<String>) -> Self {
        self.continue_custom_id = Some(custom_id.into());
        self
    }

    pub fn history(mut self, history: Vec<HistoryEntry>) -> Self {
        self.history = Some(history);
        self
    }
}

/// High-level game widget that converts to freeform
#[derive(Debug, Clone)]
pub enum GameWidget {
    Dialogue(Dialogue),
    Transcript(Transcript),
    // Future: Room, Combat, DiceRoll, etc.
}

impl GameWidget {
    /// Convert to a freeform widget with the game widgets runtime
    pub fn to_freeform(&self) -> Freeform {
        let (component_type, props_json) = match self {
            GameWidget::Dialogue(d) => ("dialogue", serde_json::to_string(d).unwrap()),
            GameWidget::Transcript(t) => ("transcript", serde_json::to_string(t).unwrap()),
        };

        Freeform::new()
            .html(r#"<div id="widget-root"></div>"#)
            .js(format!(
                r#"TulipWidgets.render(container.querySelector('#widget-root'), '{}', {}, ctx);"#,
                component_type,
                props_json
            ))
            .dependencies(game_widget_dependencies())
    }

    /// Convert to a Widget enum
    pub fn into_widget(self) -> Widget {
        Widget::Freeform(self.to_freeform())
    }
}

impl From<Dialogue> for GameWidget {
    fn from(dialogue: Dialogue) -> Self {
        GameWidget::Dialogue(dialogue)
    }
}

impl From<GameWidget> for Widget {
    fn from(game_widget: GameWidget) -> Self {
        game_widget.into_widget()
    }
}

impl From<Dialogue> for Widget {
    fn from(dialogue: Dialogue) -> Self {
        GameWidget::from(dialogue).into_widget()
    }
}

/// A transcript entry
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptEntry {
    pub speaker: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker_color: Option<String>,
}

impl TranscriptEntry {
    pub fn new(speaker: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            speaker: speaker.into(),
            text: text.into(),
            timestamp: None,
            avatar_url: None,
            speaker_color: None,
        }
    }

    pub fn with_timestamp(mut self, ts: i64) -> Self {
        self.timestamp = Some(ts);
        self
    }

    pub fn with_avatar(mut self, url: impl Into<String>) -> Self {
        self.avatar_url = Some(url.into());
        self
    }

    pub fn with_color(mut self, color: impl Into<String>) -> Self {
        self.speaker_color = Some(color.into());
        self
    }
}

/// A transcript widget for multi-speaker live conversations
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Transcript {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub entries: Vec<TranscriptEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_placeholder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_button_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_timestamps: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_scroll: Option<bool>,
}

impl Transcript {
    pub fn new() -> Self {
        Self {
            title: None,
            entries: Vec::new(),
            input_enabled: None,
            input_placeholder: None,
            input_button_text: None,
            show_timestamps: None,
            max_height: None,
            auto_scroll: None,
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn entry(mut self, entry: TranscriptEntry) -> Self {
        self.entries.push(entry);
        self
    }

    pub fn entries(mut self, entries: Vec<TranscriptEntry>) -> Self {
        self.entries = entries;
        self
    }

    pub fn with_input(mut self) -> Self {
        self.input_enabled = Some(true);
        self
    }

    pub fn input_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.input_placeholder = Some(placeholder.into());
        self
    }

    pub fn input_button_text(mut self, text: impl Into<String>) -> Self {
        self.input_button_text = Some(text.into());
        self
    }

    pub fn show_timestamps(mut self) -> Self {
        self.show_timestamps = Some(true);
        self
    }

    pub fn max_height(mut self, height: u32) -> Self {
        self.max_height = Some(height);
        self
    }

    pub fn auto_scroll(mut self, enabled: bool) -> Self {
        self.auto_scroll = Some(enabled);
        self
    }
}

impl Default for Transcript {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Transcript> for GameWidget {
    fn from(transcript: Transcript) -> Self {
        GameWidget::Transcript(transcript)
    }
}

impl From<Transcript> for Widget {
    fn from(transcript: Transcript) -> Self {
        GameWidget::from(transcript).into_widget()
    }
}
