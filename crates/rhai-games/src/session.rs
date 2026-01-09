//! Game session state management

use rhai::{Dynamic, Map};
use std::time::Instant;

/// Per-topic game session state
#[derive(Debug)]
pub struct GameSession {
    /// Script ID this session is running
    pub script_id: String,
    /// Channel/stream where this session is running
    pub channel: Option<String>,
    /// Namespace for shared state (from script metadata)
    pub namespace: Option<String>,
    /// Custom state map accessible from scripts
    pub state: Map,
    /// Turn counter
    pub turn: i64,
    /// Players in this session
    pub players: Vec<Player>,
    /// Session creation time
    pub created_at: Instant,
    /// Timer IDs owned by this session
    pub timers: Vec<u64>,
}

/// A player in a game session
#[derive(Debug, Clone)]
pub struct Player {
    pub id: i64,
    pub name: String,
    pub joined_at: Instant,
}

impl GameSession {
    /// Create a new game session
    pub fn new(script_id: String) -> Self {
        Self {
            script_id,
            channel: None,
            namespace: None,
            state: Map::new(),
            turn: 0,
            players: Vec::new(),
            created_at: Instant::now(),
            timers: Vec::new(),
        }
    }

    /// Create a new game session with channel and namespace
    pub fn with_channel(script_id: String, channel: Option<String>, namespace: Option<String>) -> Self {
        Self {
            script_id,
            channel,
            namespace,
            state: Map::new(),
            turn: 0,
            players: Vec::new(),
            created_at: Instant::now(),
            timers: Vec::new(),
        }
    }

    /// Add a timer ID to this session
    pub fn add_timer(&mut self, timer_id: u64) {
        self.timers.push(timer_id);
    }

    /// Remove a timer ID from this session
    pub fn remove_timer(&mut self, timer_id: u64) {
        self.timers.retain(|&id| id != timer_id);
    }

    /// Get a state value
    pub fn get_state(&self, key: &str) -> Dynamic {
        self.state
            .get(key)
            .cloned()
            .unwrap_or(Dynamic::UNIT)
    }

    /// Set a state value
    pub fn set_state(&mut self, key: &str, value: Dynamic) {
        self.state.insert(key.into(), value);
    }

    /// Check if a state key exists
    pub fn has_state(&self, key: &str) -> bool {
        self.state.contains_key(key)
    }

    /// Clear all state
    pub fn clear_state(&mut self) {
        self.state.clear();
        self.turn = 0;
        self.players.clear();
    }

    /// Get all state as a map
    pub fn all_state(&self) -> Map {
        self.state.clone()
    }

    /// Add a player
    pub fn add_player(&mut self, id: i64, name: String) {
        if !self.players.iter().any(|p| p.id == id) {
            self.players.push(Player {
                id,
                name,
                joined_at: Instant::now(),
            });
        }
    }

    /// Get players as a Rhai array
    pub fn players_as_dynamic(&self) -> rhai::Array {
        self.players
            .iter()
            .map(|p| {
                let mut map = Map::new();
                map.insert("id".into(), Dynamic::from(p.id));
                map.insert("name".into(), Dynamic::from(p.name.clone()));
                Dynamic::from(map)
            })
            .collect()
    }

    /// Increment turn counter and return new value
    pub fn next_turn(&mut self) -> i64 {
        self.turn += 1;
        self.turn
    }
}
