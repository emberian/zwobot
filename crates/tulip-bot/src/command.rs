//! Command system for slash commands

use crate::context::{AutocompleteContext, CommandContext};
use crate::error::Result;
use crate::response::Response;
use futures::future::BoxFuture;
use serde::Serialize;

/// A choice for command options or autocomplete
#[derive(Debug, Clone, Serialize)]
pub struct Choice {
    pub name: String,
    pub value: String,
}

impl Choice {
    /// Create a new choice
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

/// Option type for command arguments
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OptionType {
    String,
    Number,
    Boolean,
}

/// A command option/argument
#[derive(Debug, Clone, Serialize)]
pub struct CommandOption {
    pub name: String,
    #[serde(rename = "type")]
    pub option_type: OptionType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<Choice>,
}

impl CommandOption {
    /// Create a string option
    pub fn string(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            option_type: OptionType::String,
            description: Some(description.into()),
            required: false,
            choices: Vec::new(),
        }
    }

    /// Create a number option
    pub fn number(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            option_type: OptionType::Number,
            description: Some(description.into()),
            required: false,
            choices: Vec::new(),
        }
    }

    /// Create a boolean option
    pub fn boolean(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            option_type: OptionType::Boolean,
            description: Some(description.into()),
            required: false,
            choices: Vec::new(),
        }
    }

    /// Mark this option as required
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Add a static choice
    pub fn choice(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.choices.push(Choice::new(name, value));
        self
    }
}

/// Definition of a command (name, description, options)
#[derive(Debug, Clone, Serialize)]
pub struct CommandDef {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<CommandOption>,
}

impl CommandDef {
    /// Create a new command definition
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            options: Vec::new(),
        }
    }

    /// Add an option to this command
    pub fn option(mut self, opt: CommandOption) -> Self {
        self.options.push(opt);
        self
    }
}

/// Trait for implementing commands
///
/// # Example
///
/// ```ignore
/// struct WeatherCommand;
///
/// impl Command<MyData> for WeatherCommand {
///     fn definition(&self) -> CommandDef {
///         CommandDef::new("weather", "Get weather for a location")
///             .option(CommandOption::string("location", "City name").required())
///     }
///
///     fn execute<'a>(&'a self, ctx: CommandContext<'a, MyData>) -> BoxFuture<'a, Result<Response>> {
///         Box::pin(async move {
///             let location = ctx.args.get::<String>("location")?;
///             Ok(Response::message(format!("Weather in {}: Sunny!", location)))
///         })
///     }
/// }
/// ```
pub trait Command<D>: Send + Sync {
    /// Get the command definition (name, description, options)
    fn definition(&self) -> CommandDef;

    /// Execute the command
    fn execute<'a>(&'a self, ctx: CommandContext<'a, D>) -> BoxFuture<'a, Result<Response>>;

    /// Handle autocomplete requests (optional)
    ///
    /// Override this to provide dynamic autocomplete suggestions.
    fn autocomplete<'a>(
        &'a self,
        _ctx: AutocompleteContext<'a, D>,
    ) -> BoxFuture<'a, Result<Vec<Choice>>> {
        Box::pin(async { Ok(vec![]) })
    }
}
