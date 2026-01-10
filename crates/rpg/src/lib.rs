//! RPG library for zwobot
//!
//! Provides dice rolling, character stats, turn tracking, and loot tables.
//! This is a library crate - command implementations live in zwobot.

pub mod dice;
pub mod persistence;
pub mod stats;
pub mod tables;
pub mod turns;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

pub use dice::{DiceError, DiceRoll, RollResult};
pub use persistence::PersistenceError;
pub use stats::CharacterSheet;
pub use tables::roll_by_name;
pub use turns::{CombatState, Combatant};

/// Shared state for RPG features
#[derive(Clone)]
pub struct RpgData {
    /// Character stats by user_id
    pub characters: Arc<RwLock<HashMap<i64, CharacterSheet>>>,
    /// Turn tracker by topic key (channel:topic)
    pub combat: Arc<RwLock<HashMap<String, CombatState>>>,
    /// Persistence directory
    pub data_dir: PathBuf,
}

impl RpgData {
    /// Create new RPG data with the given persistence directory
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            characters: Arc::new(RwLock::new(HashMap::new())),
            combat: Arc::new(RwLock::new(HashMap::new())),
            data_dir: data_dir.into(),
        }
    }

    /// Load persisted data from disk
    pub async fn load(&self) -> Result<(), PersistenceError> {
        persistence::load_all(self).await
    }

    /// Save all data to disk
    pub async fn save(&self) -> Result<(), PersistenceError> {
        persistence::save_all(self).await
    }

    /// Get or create a character sheet for a user
    pub async fn get_or_create_character(&self, user_id: i64, name: String) -> CharacterSheet {
        let mut chars = self.characters.write().await;
        chars
            .entry(user_id)
            .or_insert_with(|| CharacterSheet::new(user_id, name))
            .clone()
    }

    /// Get a character sheet (if exists)
    pub async fn get_character(&self, user_id: i64) -> Option<CharacterSheet> {
        let chars = self.characters.read().await;
        chars.get(&user_id).cloned()
    }

    /// Update a character sheet
    pub async fn update_character(&self, sheet: CharacterSheet) {
        let mut chars = self.characters.write().await;
        chars.insert(sheet.user_id, sheet);
    }

    /// Get combat state for a topic
    pub async fn get_combat(&self, key: &str) -> Option<CombatState> {
        let combat = self.combat.read().await;
        combat.get(key).cloned()
    }

    /// Get or create combat state for a topic
    pub async fn get_or_create_combat(&self, key: &str) -> CombatState {
        let mut combat = self.combat.write().await;
        combat
            .entry(key.to_string())
            .or_insert_with(CombatState::new)
            .clone()
    }

    /// Update combat state
    pub async fn update_combat(&self, key: &str, state: CombatState) {
        let mut combat = self.combat.write().await;
        combat.insert(key.to_string(), state);
    }

    /// Remove combat state
    pub async fn remove_combat(&self, key: &str) {
        let mut combat = self.combat.write().await;
        combat.remove(key);
    }

    /// Check if combat exists for a topic
    pub async fn has_combat(&self, key: &str) -> bool {
        let combat = self.combat.read().await;
        combat.contains_key(key)
    }
}

/// Generate a topic key for combat state
pub fn combat_key(channel: &str, topic: &str) -> String {
    format!("{}:{}", channel, topic)
}
