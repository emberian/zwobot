//! Debate club game logic
//!
//! Manages debate state, generates prompts, and handles debate flow.

use std::collections::HashMap;

/// State for a single debate
#[derive(Debug, Clone, Default)]
pub struct DebateState {
    /// History of the debate: (speaker, message) pairs
    pub history: Vec<(String, String)>,
    /// Current debate topic/proposition
    pub proposition: Option<String>,
}

impl DebateState {
    /// Create a new debate with a proposition
    pub fn new(proposition: impl Into<String>) -> Self {
        Self {
            history: Vec::new(),
            proposition: Some(proposition.into()),
        }
    }

    /// Add a turn to the debate history
    pub fn add_turn(&mut self, speaker: impl Into<String>, message: impl Into<String>) {
        self.history.push((speaker.into(), message.into()));
    }

    /// Check if the debate has started
    pub fn has_started(&self) -> bool {
        self.proposition.is_some()
    }

    /// Check if there are any arguments yet
    pub fn has_arguments(&self) -> bool {
        !self.history.is_empty()
    }

    /// Get the proposition
    pub fn proposition(&self) -> Option<&str> {
        self.proposition.as_deref()
    }
}

/// Manages debates across multiple topics
#[derive(Debug, Default)]
pub struct DebateManager {
    debates: HashMap<String, DebateState>,
}

impl DebateManager {
    /// Create a new debate manager
    pub fn new() -> Self {
        Self::default()
    }

    /// Start a new debate in a topic
    pub fn start_debate(&mut self, topic: &str, proposition: &str) {
        self.debates.insert(
            topic.to_string(),
            DebateState::new(proposition),
        );
    }

    /// Get a debate by topic
    pub fn get(&self, topic: &str) -> Option<&DebateState> {
        self.debates.get(topic)
    }

    /// Get a mutable reference to a debate
    pub fn get_mut(&mut self, topic: &str) -> Option<&mut DebateState> {
        self.debates.get_mut(topic)
    }

    /// Clear a debate
    pub fn clear(&mut self, topic: &str) {
        self.debates.remove(topic);
    }

    /// Check if a topic has an active debate
    pub fn has_debate(&self, topic: &str) -> bool {
        self.debates.contains_key(topic)
    }
}

/// Build the prompt for the debate opponent
pub fn build_opponent_prompt(
    debate: &DebateState,
    human_name: &str,
    human_argument: &str,
) -> String {
    let proposition = debate.proposition().unwrap_or("the given topic");

    let mut prompt = format!(
        "You are a skilled debater arguing AGAINST the following proposition:\n\n\
         \"{}\"\n\n\
         Your opponent {} is arguing FOR this proposition.\n\n",
        proposition, human_name
    );

    // Add debate history
    if !debate.history.is_empty() {
        prompt.push_str("Previous exchanges:\n");
        for (speaker, msg) in &debate.history {
            if speaker == "Opponent" {
                prompt.push_str(&format!("You: {}\n\n", msg));
            } else {
                prompt.push_str(&format!("{}: {}\n\n", speaker, msg));
            }
        }
    }

    prompt.push_str(&format!(
        "{} just said:\n\"{}\"\n\n\
         Respond with a compelling counter-argument. Be concise but persuasive. \
         Address their specific points and make your case clearly.",
        human_name, human_argument
    ));

    prompt
}

/// Build the prompt for the judge
pub fn build_judge_prompt(debate: &DebateState) -> String {
    let proposition = debate.proposition().unwrap_or("the given topic");

    let mut prompt = format!(
        "You are an impartial judge evaluating a debate on the following proposition:\n\n\
         \"{}\"\n\n\
         Here is the complete debate:\n\n",
        proposition
    );

    for (speaker, msg) in &debate.history {
        let role = if speaker == "Opponent" {
            "AGAINST".to_string()
        } else {
            format!("FOR ({})", speaker)
        };
        prompt.push_str(&format!("[{}]: {}\n\n", role, msg));
    }

    prompt.push_str(
        "Please evaluate this debate and declare a winner. Consider:\n\
         1. Strength of arguments and evidence\n\
         2. Logical reasoning and coherence\n\
         3. Effective rebuttals of opposing points\n\
         4. Overall persuasiveness\n\n\
         Provide a brief analysis and then clearly state who won the debate and why."
    );

    prompt
}
