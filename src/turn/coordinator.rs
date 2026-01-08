use smol_str::SmolStr;
use std::collections::VecDeque;

const MAX_RECENT_ACTIONS: usize = 10;

/// A record of an action taken by a character
#[derive(Debug, Clone)]
pub struct ActionRecord {
    /// The character who took the action
    pub character: SmolStr,
    /// The turn number when the action was taken
    pub turn: u64,
    /// The tool/action used (e.g., "go north", "take sword")
    pub action: SmolStr,
    /// Human-readable summary (e.g., "Aldric moved to the forest path")
    pub summary: SmolStr,
}

/// Manages turn order and tracks recent actions for multi-character visibility
#[derive(Debug, Default, Clone)]
pub struct TurnCoordinator {
    recent_actions: VecDeque<ActionRecord>,
    current_turn: u64,
}

impl TurnCoordinator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an action taken by a character
    pub fn record_action(&mut self, character: SmolStr, action: SmolStr, summary: SmolStr) {
        self.recent_actions.push_back(ActionRecord {
            character,
            turn: self.current_turn,
            action,
            summary,
        });

        // Keep only the most recent actions
        while self.recent_actions.len() > MAX_RECENT_ACTIONS {
            self.recent_actions.pop_front();
        }
    }

    /// Advance to the next turn
    pub fn advance_turn(&mut self) {
        self.current_turn += 1;
    }

    /// Get the current turn number
    pub fn current_turn(&self) -> u64 {
        self.current_turn
    }

    /// Get recent actions for prompt context, excluding the given character
    /// Returns actions that other characters have taken, so this character can react
    pub fn recent_actions_for(&self, exclude_character: &str) -> Vec<&ActionRecord> {
        self.recent_actions
            .iter()
            .filter(|a| a.character.as_str() != exclude_character)
            .collect()
    }

    /// Get all recent actions (for debugging/logging)
    pub fn all_recent_actions(&self) -> impl Iterator<Item = &ActionRecord> {
        self.recent_actions.iter()
    }

    /// Format recent actions as a string for inclusion in prompts
    pub fn format_recent_actions_for(&self, exclude_character: &str) -> Option<String> {
        let actions = self.recent_actions_for(exclude_character);
        if actions.is_empty() {
            return None;
        }

        let formatted: Vec<String> = actions
            .iter()
            .map(|a| format!("- {}", a.summary))
            .collect();

        Some(formatted.join("\n"))
    }
}
