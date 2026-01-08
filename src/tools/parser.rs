use super::definitions::ToolCall;
use lazy_static::lazy_static;
use regex::Regex;
use smol_str::SmolStr;

lazy_static! {
    static ref TOOL_REGEX: Regex = Regex::new(r"<tool>([^<]+)</tool>").unwrap();
}

/// Parse LLM output to extract tool call
pub fn parse_tool_call(output: &str) -> anyhow::Result<ToolCall> {
    let cap = TOOL_REGEX.captures(output)
        .ok_or_else(|| anyhow::anyhow!("No tool call found in output"))?;

    let tool_str = cap.get(1).unwrap().as_str().trim();

    // Split into tool name and arguments
    let parts: Vec<&str> = tool_str.split_whitespace().collect();

    if parts.is_empty() {
        anyhow::bail!("Empty tool call");
    }

    let tool_name: SmolStr = parts[0].to_lowercase().into();
    let args: Vec<SmolStr> = parts[1..]
        .iter()
        .map(|s| (*s).into())
        .collect();

    // Extract thinking (everything before the tool tag)
    let thinking = output
        .split("<tool>")
        .next()
        .unwrap_or("")
        .trim()
        .to_string();

    Ok(ToolCall {
        thinking,
        tool_name,
        args,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_tool() {
        let output = "I should look around to understand my surroundings.\n<tool>look</tool>";
        let call = parse_tool_call(output).unwrap();

        assert_eq!(call.tool_name, "look");
        assert_eq!(call.args.len(), 0);
        assert!(call.thinking.contains("look around"));
    }

    #[test]
    fn test_parse_tool_with_args() {
        let output = "I'll examine the sword.\n<tool>examine rusty sword</tool>";
        let call = parse_tool_call(output).unwrap();

        assert_eq!(call.tool_name, "examine");
        assert_eq!(call.args.len(), 2);
        assert_eq!(call.args[0], "rusty");
        assert_eq!(call.args[1], "sword");
    }

    #[test]
    fn test_parse_tool_movement() {
        let output = "Let's head north.\n<tool>go north</tool>";
        let call = parse_tool_call(output).unwrap();

        assert_eq!(call.tool_name, "go");
        assert_eq!(call.first_arg().unwrap(), "north");
    }

    #[test]
    fn test_no_tool_call() {
        let output = "Just thinking out loud here.";
        let result = parse_tool_call(output);
        assert!(result.is_err());
    }
}
