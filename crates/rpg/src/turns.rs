//! Turn order tracking for combat

use serde::{Deserialize, Serialize};

/// A participant in combat
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Combatant {
    /// User ID (or negative for NPCs)
    pub id: i64,
    /// Display name
    pub name: String,
    /// Initiative roll result
    pub initiative: i32,
    /// Whether this is an NPC
    pub is_npc: bool,
}

/// State of a combat encounter
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CombatState {
    /// Participants sorted by initiative (highest first)
    pub combatants: Vec<Combatant>,
    /// Index of current turn (None if combat hasn't started)
    pub current_turn: Option<usize>,
    /// Current round number
    pub round: u32,
    /// Whether combat is active
    pub active: bool,
}

impl CombatState {
    /// Create a new combat state
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a combatant and sort by initiative
    pub fn add_combatant(&mut self, id: i64, name: String, initiative: i32, is_npc: bool) {
        // Remove if already exists
        self.combatants.retain(|c| c.id != id);

        self.combatants.push(Combatant {
            id,
            name,
            initiative,
            is_npc,
        });

        // Sort by initiative (highest first), then by name for ties
        self.combatants.sort_by(|a, b| {
            b.initiative
                .cmp(&a.initiative)
                .then_with(|| a.name.cmp(&b.name))
        });
    }

    /// Remove a combatant by ID
    pub fn remove_combatant(&mut self, id: i64) -> Option<Combatant> {
        if let Some(pos) = self.combatants.iter().position(|c| c.id == id) {
            // Adjust current turn if needed
            if let Some(current) = self.current_turn {
                if pos < current {
                    self.current_turn = Some(current - 1);
                } else if pos == current && current >= self.combatants.len() - 1 {
                    self.current_turn = Some(0);
                }
            }
            Some(self.combatants.remove(pos))
        } else {
            None
        }
    }

    /// Start combat
    pub fn start(&mut self) {
        if !self.combatants.is_empty() {
            self.active = true;
            self.current_turn = Some(0);
            self.round = 1;
        }
    }

    /// Advance to next turn
    pub fn next_turn(&mut self) -> Option<&Combatant> {
        if !self.active || self.combatants.is_empty() {
            return None;
        }

        if let Some(current) = self.current_turn {
            let next = (current + 1) % self.combatants.len();
            if next == 0 {
                self.round += 1;
            }
            self.current_turn = Some(next);
            self.combatants.get(next)
        } else {
            self.current_turn = Some(0);
            self.combatants.first()
        }
    }

    /// Get current combatant
    pub fn current(&self) -> Option<&Combatant> {
        self.current_turn.and_then(|i| self.combatants.get(i))
    }

    /// End combat
    pub fn end(&mut self) {
        self.active = false;
        self.current_turn = None;
        self.combatants.clear();
        self.round = 0;
    }

    /// Check if a user is in combat
    pub fn has_combatant(&self, id: i64) -> bool {
        self.combatants.iter().any(|c| c.id == id)
    }

    /// Format turn order for display
    pub fn format_turn_order(&self) -> String {
        if self.combatants.is_empty() {
            return "No combatants".to_string();
        }

        let mut lines = Vec::new();
        for (i, c) in self.combatants.iter().enumerate() {
            let marker = if Some(i) == self.current_turn {
                "▶ "
            } else {
                "  "
            };
            let npc_tag = if c.is_npc { " [NPC]" } else { "" };
            lines.push(format!("{}{} ({}){}", marker, c.name, c.initiative, npc_tag));
        }
        lines.join("\n")
    }
}
