//! Character stat tracking

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A character's stats
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CharacterSheet {
    /// Character name (defaults to user's display name)
    pub name: String,
    /// User ID who owns this character
    pub user_id: i64,
    /// HP: current and max
    pub hp: i32,
    pub hp_max: i32,
    /// Core stats (str, dex, con, int, wis, cha)
    pub stats: HashMap<String, i32>,
    /// Arbitrary key-value storage
    pub custom: HashMap<String, String>,
}

impl CharacterSheet {
    /// Create a new character sheet
    pub fn new(user_id: i64, name: String) -> Self {
        Self {
            name,
            user_id,
            hp: 10,
            hp_max: 10,
            stats: HashMap::new(),
            custom: HashMap::new(),
        }
    }

    /// Get a stat modifier (D&D style: (stat - 10) / 2)
    pub fn modifier(&self, stat: &str) -> i32 {
        self.stats.get(stat).map(|&v| (v - 10) / 2).unwrap_or(0)
    }

    /// Take damage (returns remaining HP)
    pub fn hurt(&mut self, amount: i32) -> i32 {
        self.hp = (self.hp - amount).max(0);
        self.hp
    }

    /// Heal (capped at max HP, returns new HP)
    pub fn heal(&mut self, amount: i32) -> i32 {
        self.hp = (self.hp + amount).min(self.hp_max);
        self.hp
    }

    /// Set a stat value
    pub fn set_stat(&mut self, stat: &str, value: i32) {
        // Handle special stats
        match stat.to_lowercase().as_str() {
            "hp" => self.hp = value,
            "hp_max" | "maxhp" | "max_hp" => self.hp_max = value,
            s => {
                self.stats.insert(s.to_string(), value);
            }
        }
    }

    /// Get a stat value
    pub fn get_stat(&self, stat: &str) -> Option<i32> {
        match stat.to_lowercase().as_str() {
            "hp" => Some(self.hp),
            "hp_max" | "maxhp" | "max_hp" => Some(self.hp_max),
            s => self.stats.get(s).copied(),
        }
    }

    /// Format HP as a progress bar
    pub fn hp_bar(&self) -> String {
        if self.hp_max <= 0 {
            return format!("HP: {}", self.hp);
        }
        let pct = (self.hp as f32 / self.hp_max as f32 * 10.0).round() as usize;
        let filled = pct.min(10);
        let empty = 10 - filled;
        format!(
            "HP: {} {}/{}",
            "█".repeat(filled) + &"░".repeat(empty),
            self.hp,
            self.hp_max
        )
    }

    /// Format a stat with its modifier
    pub fn format_stat(&self, stat: &str) -> Option<String> {
        self.stats.get(stat).map(|&v| {
            let m = (v - 10) / 2;
            let sign = if m >= 0 { "+" } else { "" };
            format!("{}: {} ({}{})", stat.to_uppercase(), v, sign, m)
        })
    }
}
