//! Interactive components (buttons, select menus, modals)

use serde::Serialize;

/// Interactive widget with components
#[derive(Debug, Clone, Default, Serialize)]
pub struct Interactive {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    pub components: Vec<ActionRow>,
}

impl Interactive {
    /// Create a new interactive widget
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the content text
    pub fn content(mut self, content: impl Into<String>) -> Self {
        self.content = Some(content.into());
        self
    }

    /// Add an action row
    pub fn row(mut self, row: ActionRow) -> Self {
        self.components.push(row);
        self
    }
}

/// A row of components
#[derive(Debug, Clone, Default, Serialize)]
pub struct ActionRow {
    #[serde(rename = "type")]
    pub row_type: &'static str,
    pub components: Vec<Component>,
}

impl ActionRow {
    /// Create a new action row
    pub fn new() -> Self {
        Self {
            row_type: "action_row",
            components: Vec::new(),
        }
    }

    /// Add a button
    pub fn button(mut self, button: Button) -> Self {
        self.components.push(Component::Button(button));
        self
    }

    /// Add a select menu
    pub fn select_menu(mut self, menu: SelectMenu) -> Self {
        self.components.push(Component::SelectMenu(menu));
        self
    }

    /// Add a text input (for modals)
    pub fn text_input(mut self, input: TextInput) -> Self {
        self.components.push(Component::TextInput(input));
        self
    }
}

/// A component in an action row
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum Component {
    Button(Button),
    SelectMenu(SelectMenu),
    TextInput(TextInput),
}

/// Button styles
#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ButtonStyle {
    Primary,
    #[default]
    Secondary,
    Success,
    Danger,
    Link,
}

/// A button component
#[derive(Debug, Clone, Serialize)]
pub struct Button {
    #[serde(rename = "type")]
    pub component_type: &'static str,
    pub label: String,
    #[serde(skip_serializing_if = "is_default_style")]
    pub style: ButtonStyle,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub disabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modal: Option<Modal>,
}

fn is_default_style(style: &ButtonStyle) -> bool {
    matches!(style, ButtonStyle::Secondary)
}

impl Button {
    /// Create a new button with a custom_id
    pub fn new(label: impl Into<String>, custom_id: impl Into<String>) -> Self {
        Self {
            component_type: "button",
            label: label.into(),
            style: ButtonStyle::Secondary,
            custom_id: Some(custom_id.into()),
            url: None,
            disabled: false,
            modal: None,
        }
    }

    /// Create a link button
    pub fn link(label: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            component_type: "button",
            label: label.into(),
            style: ButtonStyle::Link,
            custom_id: None,
            url: Some(url.into()),
            disabled: false,
            modal: None,
        }
    }

    /// Set the style
    pub fn style(mut self, style: ButtonStyle) -> Self {
        self.style = style;
        self
    }

    /// Set as primary style
    pub fn primary(mut self) -> Self {
        self.style = ButtonStyle::Primary;
        self
    }

    /// Set as success style
    pub fn success(mut self) -> Self {
        self.style = ButtonStyle::Success;
        self
    }

    /// Set as danger style
    pub fn danger(mut self) -> Self {
        self.style = ButtonStyle::Danger;
        self
    }

    /// Set disabled state
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Attach a modal to open when clicked
    pub fn with_modal(mut self, modal: Modal) -> Self {
        self.modal = Some(modal);
        self
    }
}

/// A select menu option
#[derive(Debug, Clone, Serialize)]
pub struct SelectOption {
    pub label: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub default: bool,
}

impl SelectOption {
    /// Create a new option
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            description: None,
            default: false,
        }
    }

    /// Add a description
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set as default selected
    pub fn default(mut self) -> Self {
        self.default = true;
        self
    }
}

/// A select menu component
#[derive(Debug, Clone, Serialize)]
pub struct SelectMenu {
    #[serde(rename = "type")]
    pub component_type: &'static str,
    pub custom_id: String,
    pub options: Vec<SelectOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(skip_serializing_if = "is_one")]
    pub min_values: u32,
    #[serde(skip_serializing_if = "is_one")]
    pub max_values: u32,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub disabled: bool,
}

fn is_one(v: &u32) -> bool {
    *v == 1
}

impl SelectMenu {
    /// Create a new select menu
    pub fn new(custom_id: impl Into<String>) -> Self {
        Self {
            component_type: "select_menu",
            custom_id: custom_id.into(),
            options: Vec::new(),
            placeholder: None,
            min_values: 1,
            max_values: 1,
            disabled: false,
        }
    }

    /// Add an option
    pub fn option(mut self, option: SelectOption) -> Self {
        self.options.push(option);
        self
    }

    /// Set the placeholder text
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Set the number of values that can be selected
    pub fn values(mut self, min: u32, max: u32) -> Self {
        self.min_values = min;
        self.max_values = max;
        self
    }

    /// Set disabled state
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// A modal dialog
#[derive(Debug, Clone, Serialize)]
pub struct Modal {
    pub custom_id: String,
    pub title: String,
    pub components: Vec<ActionRow>,
}

impl Modal {
    /// Create a new modal
    pub fn new(custom_id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            custom_id: custom_id.into(),
            title: title.into(),
            components: Vec::new(),
        }
    }

    /// Add a row (usually containing a TextInput)
    pub fn row(mut self, row: ActionRow) -> Self {
        self.components.push(row);
        self
    }
}

/// Text input style
#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TextInputStyle {
    #[default]
    Short,
    Paragraph,
}

/// A text input component (for modals)
#[derive(Debug, Clone, Serialize)]
pub struct TextInput {
    #[serde(rename = "type")]
    pub component_type: &'static str,
    pub custom_id: String,
    pub label: String,
    #[serde(skip_serializing_if = "is_default_text_style")]
    pub style: TextInputStyle,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_length: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub required: bool,
}

fn is_default_text_style(style: &TextInputStyle) -> bool {
    matches!(style, TextInputStyle::Short)
}

impl TextInput {
    /// Create a new text input
    pub fn new(custom_id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            component_type: "text_input",
            custom_id: custom_id.into(),
            label: label.into(),
            style: TextInputStyle::Short,
            placeholder: None,
            value: None,
            min_length: None,
            max_length: None,
            required: false,
        }
    }

    /// Set as paragraph (multi-line) style
    pub fn paragraph(mut self) -> Self {
        self.style = TextInputStyle::Paragraph;
        self
    }

    /// Set the placeholder text
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Set a pre-filled value
    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    /// Set length constraints
    pub fn length(mut self, min: Option<u32>, max: Option<u32>) -> Self {
        self.min_length = min;
        self.max_length = max;
        self
    }

    /// Set as required
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }
}
