use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::collections::HashMap;

/// Complete game world state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldState {
    /// The spatial world (rooms, exits, objects)
    pub spatial: SpatialWorld,
    /// Per-character state (location, inventory, stats, etc.)
    pub characters: HashMap<SmolStr, CharacterState>,
    /// Object definitions (items that can exist in the world)
    pub object_defs: HashMap<SmolStr, WorldObject>,
    /// NPC definitions
    pub npc_defs: HashMap<SmolStr, Npc>,
    /// Global variables for scene effects
    pub variables: HashMap<SmolStr, Value>,
    /// Current turn number
    pub turn_number: u64,
}

impl WorldState {
    /// Create a new empty world state
    pub fn new() -> Self {
        Self {
            spatial: SpatialWorld::new(),
            characters: HashMap::new(),
            object_defs: HashMap::new(),
            npc_defs: HashMap::new(),
            variables: HashMap::new(),
            turn_number: 0,
        }
    }

    /// Get a character's state
    pub fn get_character(&self, name: &str) -> Option<&CharacterState> {
        self.characters.get(name)
    }

    /// Get a mutable character's state
    pub fn get_character_mut(&mut self, name: &str) -> Option<&mut CharacterState> {
        self.characters.get_mut(name)
    }

    /// Add a new character to the world
    pub fn add_character(&mut self, name: SmolStr, location: SmolStr) {
        let char_state = CharacterState {
            location: location.clone(),
            inventory: Vec::new(),
            stats: CharacterStats::default(),
            equipment: Equipment::default(),
        };

        self.characters.insert(name.clone(), char_state);

        // Add character to room
        if let Some(room) = self.spatial.rooms.get_mut(&location) {
            room.characters.push(name);
        }
    }

    /// Get the room a character is currently in
    pub fn get_character_room(&self, character: &str) -> Option<&Room> {
        let char_state = self.get_character(character)?;
        self.spatial.rooms.get(&char_state.location)
    }

    /// Move a character to a new room
    pub fn move_character(&mut self, character: &SmolStr, new_room: SmolStr) -> anyhow::Result<()> {
        let char_state = self.get_character_mut(character)
            .ok_or_else(|| anyhow::anyhow!("Character {} not found", character))?;

        let old_room = char_state.location.clone();
        char_state.location = new_room.clone();

        // Remove from old room
        if let Some(room) = self.spatial.rooms.get_mut(&old_room) {
            room.characters.retain(|c| c != character);
        }

        // Add to new room
        if let Some(room) = self.spatial.rooms.get_mut(&new_room) {
            room.characters.push(character.clone());
        }

        Ok(())
    }
}

/// The spatial world structure (rooms and connections)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpatialWorld {
    /// All rooms in the world, keyed by room ID
    pub rooms: HashMap<SmolStr, Room>,
}

impl SpatialWorld {
    pub fn new() -> Self {
        Self {
            rooms: HashMap::new(),
        }
    }

    pub fn add_room(&mut self, id: SmolStr, room: Room) {
        self.rooms.insert(id, room);
    }

    pub fn get_room(&self, id: &str) -> Option<&Room> {
        self.rooms.get(id)
    }

    pub fn get_room_mut(&mut self, id: &str) -> Option<&mut Room> {
        self.rooms.get_mut(id)
    }
}

/// A room in the world
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    /// Room name
    pub name: SmolStr,
    /// Room description
    pub description: SmolStr,
    /// Exits to other rooms
    pub exits: HashMap<SmolStr, Exit>,
    /// Objects in this room
    pub objects: Vec<SmolStr>,
    /// NPCs in this room
    pub npcs: Vec<SmolStr>,
    /// Characters currently in this room
    pub characters: Vec<SmolStr>,
}

impl Room {
    pub fn new(name: SmolStr, description: SmolStr) -> Self {
        Self {
            name,
            description,
            exits: HashMap::new(),
            objects: Vec::new(),
            npcs: Vec::new(),
            characters: Vec::new(),
        }
    }

    /// Add an exit to this room
    pub fn add_exit(&mut self, direction: SmolStr, target: SmolStr) {
        self.exits.insert(direction, Exit {
            target_room: target,
            requires: None,
        });
    }

    /// Add an exit with a requirement
    pub fn add_exit_with_requirement(&mut self, direction: SmolStr, target: SmolStr, requires: Condition) {
        self.exits.insert(direction, Exit {
            target_room: target,
            requires: Some(requires),
        });
    }

    /// Add an object to this room
    pub fn add_object(&mut self, object_id: SmolStr) {
        self.objects.push(object_id);
    }

    /// Remove an object from this room
    pub fn remove_object(&mut self, object_id: &str) -> bool {
        if let Some(pos) = self.objects.iter().position(|o| o == object_id) {
            self.objects.remove(pos);
            true
        } else {
            false
        }
    }

    /// Add an NPC to this room
    pub fn add_npc(&mut self, npc_id: SmolStr) {
        self.npcs.push(npc_id);
    }
}

/// An exit from a room
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exit {
    /// Target room ID
    pub target_room: SmolStr,
    /// Optional condition required to use this exit
    pub requires: Option<Condition>,
}

/// Per-character state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterState {
    /// Current room location
    pub location: SmolStr,
    /// Character's inventory (object IDs)
    pub inventory: Vec<SmolStr>,
    /// Character stats (health, mana, etc.)
    pub stats: CharacterStats,
    /// Equipped items
    pub equipment: Equipment,
}

/// Character statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterStats {
    pub health: i32,
    pub max_health: i32,
    pub mana: i32,
    pub max_mana: i32,
    pub strength: i32,
    pub intelligence: i32,
    pub dexterity: i32,
}

impl Default for CharacterStats {
    fn default() -> Self {
        Self {
            health: 100,
            max_health: 100,
            mana: 50,
            max_mana: 50,
            strength: 10,
            intelligence: 10,
            dexterity: 10,
        }
    }
}

/// Equipped items
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Equipment {
    pub weapon: Option<SmolStr>,
    pub armor: Option<SmolStr>,
    pub accessory: Option<SmolStr>,
}

/// A world object (item)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldObject {
    /// Object ID
    pub id: SmolStr,
    /// Display name
    pub name: SmolStr,
    /// Description
    pub description: SmolStr,
    /// Tags for categorization and behavior
    pub tags: Vec<SmolStr>,
    /// Whether this object can be taken
    pub takeable: bool,
    /// Whether this object can be equipped
    pub equippable: bool,
    /// Equipment slot (if equippable)
    pub slot: Option<EquipmentSlot>,
    /// Stat modifiers when equipped
    pub stat_mods: HashMap<SmolStr, i32>,
}

/// Equipment slots
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EquipmentSlot {
    Weapon,
    Armor,
    Accessory,
}

/// An NPC in the world
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Npc {
    /// NPC ID
    pub id: SmolStr,
    /// Display name
    pub name: SmolStr,
    /// Description
    pub description: SmolStr,
    /// Dialogue scene (if any)
    pub dialogue_scene: Option<SmolStr>,
    /// Health (for combat)
    pub health: i32,
    /// Max health
    pub max_health: i32,
    /// Whether hostile
    pub hostile: bool,
}

/// Conditions for exits, scene triggers, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Condition {
    /// Has item in inventory
    HasItem(SmolStr),
    /// Has variable set to value
    Variable { name: SmolStr, value: Value },
    /// Multiple conditions (all must be true)
    And(Vec<Condition>),
    /// Multiple conditions (any can be true)
    Or(Vec<Condition>),
}

/// Generic value type for variables
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Bool(bool),
    Int(i32),
    String(SmolStr),
}

