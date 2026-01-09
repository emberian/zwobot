//! Script representation and metadata parsing

use rhai::AST;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use crate::LlmConfig;

/// A compiled Rhai game script with metadata
#[derive(Clone)]
pub struct GameScript {
    /// Script identifier (filename without extension)
    pub id: String,
    /// Path to source file (for hot reload)
    pub path: PathBuf,
    /// Compiled AST
    pub ast: Arc<AST>,
    /// Script metadata extracted from header
    pub meta: ScriptMeta,
    /// Last modification time
    pub modified: SystemTime,
}

/// Metadata extracted from script header comments
#[derive(Clone, Debug, Default)]
pub struct ScriptMeta {
    /// Display name for the game
    pub title: String,
    /// Description
    pub description: String,
    /// Commands this script registers
    pub commands: Vec<CommandMeta>,
    /// Whether this script handles messages (has `on_message` function)
    pub handles_messages: bool,
    /// Default LLM config if script uses LLM
    pub default_llm: Option<LlmConfig>,
}

/// Metadata for a script command
#[derive(Clone, Debug)]
pub struct CommandMeta {
    pub name: String,
    pub description: String,
    pub options: Vec<OptionMeta>,
}

/// Metadata for a command option/argument
#[derive(Clone, Debug)]
pub struct OptionMeta {
    pub name: String,
    pub description: String,
    pub option_type: String, // "string", "number", "boolean"
    pub required: bool,
}

impl GameScript {
    /// Parse metadata from script header comments
    ///
    /// Format:
    /// ```text
    /// // @title Debate Game
    /// // @description AI-powered debate opponent
    /// // @command debate(proposition: string!) - Start a new debate
    /// // @command judge() - Request judgment
    /// // @llm model=bartowski/Llama-3.2-1B-Instruct-GGUF quant=Q8_0 tokens=1024 temp=0.7
    /// ```
    pub fn parse_meta(source: &str) -> ScriptMeta {
        let mut meta = ScriptMeta::default();

        for line in source.lines() {
            let line = line.trim();
            if !line.starts_with("//") {
                // Stop at first non-comment line
                if !line.is_empty() {
                    break;
                }
                continue;
            }

            let content = line.trim_start_matches("//").trim();

            if let Some(rest) = content.strip_prefix("@title ") {
                meta.title = rest.trim().to_string();
            } else if let Some(rest) = content.strip_prefix("@description ") {
                meta.description = rest.trim().to_string();
            } else if let Some(rest) = content.strip_prefix("@command ") {
                if let Some(cmd) = Self::parse_command_meta(rest) {
                    meta.commands.push(cmd);
                }
            } else if let Some(rest) = content.strip_prefix("@llm ") {
                meta.default_llm = Self::parse_llm_config(rest);
            }
        }

        // Check if script has on_message function
        meta.handles_messages = source.contains("fn on_message(");

        meta
    }

    /// Parse a command annotation like `debate(proposition: string!) - Start a debate`
    fn parse_command_meta(input: &str) -> Option<CommandMeta> {
        // Split on " - " to get command signature and description
        let (signature, description) = if let Some(idx) = input.find(" - ") {
            (&input[..idx], input[idx + 3..].trim())
        } else {
            (input, "")
        };

        // Parse command name and arguments
        let signature = signature.trim();
        let (name, args_str) = if let Some(paren_idx) = signature.find('(') {
            let name = &signature[..paren_idx];
            let args = signature[paren_idx + 1..].trim_end_matches(')');
            (name.trim(), args)
        } else {
            (signature, "")
        };

        if name.is_empty() {
            return None;
        }

        // Parse arguments
        let options = Self::parse_options(args_str);

        Some(CommandMeta {
            name: name.to_string(),
            description: description.to_string(),
            options,
        })
    }

    /// Parse options from argument list like `proposition: string!, count: number`
    fn parse_options(args_str: &str) -> Vec<OptionMeta> {
        if args_str.trim().is_empty() {
            return Vec::new();
        }

        args_str
            .split(',')
            .filter_map(|arg| {
                let arg = arg.trim();
                if arg.is_empty() {
                    return None;
                }

                // Parse "name: type!" format
                let (name, type_str) = if let Some(colon_idx) = arg.find(':') {
                    (&arg[..colon_idx], arg[colon_idx + 1..].trim())
                } else {
                    (arg, "string")
                };

                let required = type_str.ends_with('!');
                let option_type = type_str.trim_end_matches('!').trim().to_string();

                Some(OptionMeta {
                    name: name.trim().to_string(),
                    description: String::new(),
                    option_type: if option_type.is_empty() {
                        "string".to_string()
                    } else {
                        option_type
                    },
                    required,
                })
            })
            .collect()
    }

    /// Parse LLM config from annotation like `model=foo quant=Q8_0 tokens=1024 temp=0.7`
    fn parse_llm_config(input: &str) -> Option<LlmConfig> {
        let mut model_id = String::new();
        let mut quantization = String::from("Q4_K_M");
        let mut max_tokens = 1024u32;
        let mut temperature = 0.7f32;

        for part in input.split_whitespace() {
            if let Some((key, value)) = part.split_once('=') {
                match key {
                    "model" => model_id = value.to_string(),
                    "quant" => quantization = value.to_string(),
                    "tokens" => max_tokens = value.parse().unwrap_or(1024),
                    "temp" => temperature = value.parse().unwrap_or(0.7),
                    _ => {}
                }
            }
        }

        if model_id.is_empty() {
            return None;
        }

        Some(LlmConfig {
            model_id,
            quantization,
            max_tokens,
            temperature,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_meta() {
        let source = r#"// @title Debate Club
// @description AI-powered debate opponent
// @command debate(proposition: string!) - Start a new debate
// @command judge() - Request judgment
// @llm model=bartowski/Llama-3.2-1B-Instruct-GGUF quant=Q8_0 tokens=512 temp=0.8

fn on_command_debate(proposition) {
    // ...
}

fn on_message() {
    // ...
}
"#;

        let meta = GameScript::parse_meta(source);
        assert_eq!(meta.title, "Debate Club");
        assert_eq!(meta.description, "AI-powered debate opponent");
        assert_eq!(meta.commands.len(), 2);
        assert_eq!(meta.commands[0].name, "debate");
        assert_eq!(meta.commands[0].options.len(), 1);
        assert_eq!(meta.commands[0].options[0].name, "proposition");
        assert!(meta.commands[0].options[0].required);
        assert_eq!(meta.commands[1].name, "judge");
        assert!(meta.commands[1].options.is_empty());
        assert!(meta.handles_messages);

        let llm = meta.default_llm.unwrap();
        assert_eq!(llm.model_id, "bartowski/Llama-3.2-1B-Instruct-GGUF");
        assert_eq!(llm.quantization, "Q8_0");
        assert_eq!(llm.max_tokens, 512);
        assert!((llm.temperature - 0.8).abs() < 0.01);
    }
}
