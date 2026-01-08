//! EffectHandler implementation connecting spween to WorldState.

use crate::world::WorldState;
use smol_str::SmolStr;
use spween::{EffectHandler, Value};
use tracing::info;

/// Context for executing scene effects, wrapping WorldState + active character.
pub struct SceneContext<'a> {
    pub world: &'a mut WorldState,
    pub character: SmolStr,
}

impl<'a> SceneContext<'a> {
    pub fn new(world: &'a mut WorldState, character: SmolStr) -> Self {
        Self { world, character }
    }
}

impl EffectHandler for SceneContext<'_> {
    fn get_var(&self, name: &str) -> Value {
        // First check character stats
        if let Some(char_state) = self.world.get_character(&self.character) {
            match name {
                "health" => return Value::Int(char_state.stats.health as i64),
                "max_health" => return Value::Int(char_state.stats.max_health as i64),
                "mana" => return Value::Int(char_state.stats.mana as i64),
                "max_mana" => return Value::Int(char_state.stats.max_mana as i64),
                "strength" => return Value::Int(char_state.stats.strength as i64),
                "intelligence" => return Value::Int(char_state.stats.intelligence as i64),
                "dexterity" => return Value::Int(char_state.stats.dexterity as i64),
                _ => {}
            }
        }

        // Then check global variables
        if let Some(value) = self.world.variables.get(name) {
            return convert_world_value_to_spween(value);
        }

        Value::Null
    }

    fn set_var(&mut self, name: &str, value: Value) {
        // Check if it's a character stat
        if let Some(char_state) = self.world.get_character_mut(&self.character) {
            match name {
                "health" => {
                    if let Some(n) = value.as_int() {
                        char_state.stats.health = n.clamp(0, char_state.stats.max_health as i64) as i32;
                        return;
                    }
                }
                "mana" => {
                    if let Some(n) = value.as_int() {
                        char_state.stats.mana = n.clamp(0, char_state.stats.max_mana as i64) as i32;
                        return;
                    }
                }
                "strength" => {
                    if let Some(n) = value.as_int() {
                        char_state.stats.strength = n as i32;
                        return;
                    }
                }
                "intelligence" => {
                    if let Some(n) = value.as_int() {
                        char_state.stats.intelligence = n as i32;
                        return;
                    }
                }
                "dexterity" => {
                    if let Some(n) = value.as_int() {
                        char_state.stats.dexterity = n as i32;
                        return;
                    }
                }
                _ => {}
            }
        }

        // Store in global variables
        self.world.variables.insert(
            SmolStr::new(name),
            convert_spween_value_to_world(&value),
        );
    }

    fn has(&self, category: &str, key: &str) -> bool {
        match category {
            "inventory" => {
                // Check if character has item in inventory
                if let Some(char_state) = self.world.get_character(&self.character) {
                    char_state.inventory.iter().any(|id| {
                        // Match by object ID or name
                        if id.as_str() == key {
                            return true;
                        }
                        if let Some(obj) = self.world.object_defs.get(id) {
                            obj.name.to_lowercase().contains(&key.to_lowercase())
                        } else {
                            false
                        }
                    })
                } else {
                    false
                }
            }
            "equipped" => {
                // Check if character has item equipped
                if let Some(char_state) = self.world.get_character(&self.character) {
                    let key_lower = key.to_lowercase();
                    char_state.equipment.weapon.as_ref().map_or(false, |id| {
                        id.as_str() == key || self.world.object_defs.get(id).map_or(false, |o| o.name.to_lowercase().contains(&key_lower))
                    }) || char_state.equipment.armor.as_ref().map_or(false, |id| {
                        id.as_str() == key || self.world.object_defs.get(id).map_or(false, |o| o.name.to_lowercase().contains(&key_lower))
                    }) || char_state.equipment.accessory.as_ref().map_or(false, |id| {
                        id.as_str() == key || self.world.object_defs.get(id).map_or(false, |o| o.name.to_lowercase().contains(&key_lower))
                    })
                } else {
                    false
                }
            }
            "tag" => {
                // Check if character has an item with a specific tag
                if let Some(char_state) = self.world.get_character(&self.character) {
                    char_state.inventory.iter().any(|id| {
                        self.world.object_defs.get(id).map_or(false, |obj| {
                            obj.tags.iter().any(|t| t.as_str() == key)
                        })
                    })
                } else {
                    false
                }
            }
            "flag" => {
                // Check global flag variable
                self.world.variables.get(key).map_or(false, |v| {
                    matches!(v, crate::world::Value::Bool(true))
                })
            }
            _ => false,
        }
    }

    fn call(&mut self, name: &str, args: &[Value]) -> Result<(), String> {
        match name {
            "give_item" => {
                // give_item "item_id"
                let item_id = args.first()
                    .and_then(|v| v.as_str())
                    .ok_or("give_item requires item ID")?;

                if let Some(char_state) = self.world.get_character_mut(&self.character) {
                    char_state.inventory.push(SmolStr::new(item_id));
                    info!("Scene effect: gave {} to {}", item_id, self.character);
                }
                Ok(())
            }
            "take_item" => {
                // take_item "item_id" - remove from inventory
                let item_id = args.first()
                    .and_then(|v| v.as_str())
                    .ok_or("take_item requires item ID")?;

                if let Some(char_state) = self.world.get_character_mut(&self.character) {
                    char_state.inventory.retain(|id| id.as_str() != item_id);
                    info!("Scene effect: took {} from {}", item_id, self.character);
                }
                Ok(())
            }
            "heal" => {
                // heal 25 - heal character
                let amount = args.first()
                    .and_then(|v| v.as_int())
                    .unwrap_or(25) as i32;

                if let Some(char_state) = self.world.get_character_mut(&self.character) {
                    char_state.stats.health = (char_state.stats.health + amount).min(char_state.stats.max_health);
                    info!("Scene effect: healed {} for {} HP", self.character, amount);
                }
                Ok(())
            }
            "damage" => {
                // damage 10 - damage character
                let amount = args.first()
                    .and_then(|v| v.as_int())
                    .unwrap_or(10) as i32;

                if let Some(char_state) = self.world.get_character_mut(&self.character) {
                    char_state.stats.health = (char_state.stats.health - amount).max(0);
                    info!("Scene effect: damaged {} for {} HP", self.character, amount);
                }
                Ok(())
            }
            "set_flag" => {
                // set_flag "flag_name" - set global flag to true
                let flag = args.first()
                    .and_then(|v| v.as_str())
                    .ok_or("set_flag requires flag name")?;

                self.world.variables.insert(SmolStr::new(flag), crate::world::Value::Bool(true));
                info!("Scene effect: set flag {}", flag);
                Ok(())
            }
            "clear_flag" => {
                // clear_flag "flag_name" - remove flag
                let flag = args.first()
                    .and_then(|v| v.as_str())
                    .ok_or("clear_flag requires flag name")?;

                self.world.variables.remove(flag);
                info!("Scene effect: cleared flag {}", flag);
                Ok(())
            }
            "teleport" => {
                // teleport "room_id" - move character to room
                let room_id = args.first()
                    .and_then(|v| v.as_str())
                    .ok_or("teleport requires room ID")?;

                let char = self.character.clone();
                self.world.move_character(&char, SmolStr::new(room_id))
                    .map_err(|e| e.to_string())?;
                info!("Scene effect: teleported {} to {}", self.character, room_id);
                Ok(())
            }
            _ => {
                // Unknown effect - log but don't fail
                info!("Unknown scene effect: {} {:?}", name, args);
                Ok(())
            }
        }
    }
}

fn convert_world_value_to_spween(value: &crate::world::Value) -> Value {
    match value {
        crate::world::Value::Bool(b) => Value::Bool(*b),
        crate::world::Value::Int(n) => Value::Int(*n as i64),
        crate::world::Value::String(s) => Value::String(s.clone()),
    }
}

fn convert_spween_value_to_world(value: &Value) -> crate::world::Value {
    match value {
        Value::Null => crate::world::Value::Bool(false),
        Value::Bool(b) => crate::world::Value::Bool(*b),
        Value::Int(n) => crate::world::Value::Int(*n as i32),
        Value::Float(f) => crate::world::Value::Int(*f as i32),
        Value::String(s) => crate::world::Value::String(s.clone()),
    }
}
