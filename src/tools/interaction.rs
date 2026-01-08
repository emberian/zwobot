use super::definitions::{ToolCall, ToolResult};
use super::matching::matches_name;
use crate::world::WorldState;
use smol_str::SmolStr;

/// Execute the 'talk' tool - talk to an NPC in the current room
pub fn tool_talk(
    world: &WorldState,
    character: &SmolStr,
    call: &ToolCall,
) -> anyhow::Result<ToolResult> {
    let target = call.args_joined();
    if target.is_empty() {
        return Ok(ToolResult::failure("Talk to whom?".to_string()));
    }

    let char_state = world
        .get_character(character)
        .ok_or_else(|| anyhow::anyhow!("Character {} not found", character))?;

    let room = world
        .spatial
        .rooms
        .get(&char_state.location)
        .ok_or_else(|| anyhow::anyhow!("Room not found"))?;

    // Find matching NPC in room
    let mut found_npc_id: Option<&SmolStr> = None;

    for npc_id in &room.npcs {
        if let Some(npc) = world.npc_defs.get(npc_id) {
            if matches_name(&npc.name, &target) {
                found_npc_id = Some(npc_id);
                break;
            }
        }
    }

    let npc_id = found_npc_id.ok_or_else(|| anyhow::anyhow!("'{}' is not here.", target))?;

    let npc = world
        .npc_defs
        .get(npc_id)
        .ok_or_else(|| anyhow::anyhow!("NPC not found"))?;

    // TODO Phase 5: Check for dialogue_scene and load spween scene
    // For now, generate simple dialogue from NPC description

    let greeting = generate_npc_greeting(&npc.description);

    Ok(ToolResult::success(
        format!(
            "{} turns to you.\n\n\"{}\"\n\n*{}*",
            npc.name, greeting, npc.description
        ),
        format!("Spoke with {}", npc.name),
    ))
}

/// Generate a simple greeting based on NPC description
fn generate_npc_greeting(description: &SmolStr) -> String {
    // Extract personality hints from description for simple dialogue
    let desc_lower = description.to_lowercase();

    if desc_lower.contains("warm") || desc_lower.contains("friendly") {
        "Welcome, traveler! What can I do for you today?".to_string()
    } else if desc_lower.contains("shrewd") || desc_lower.contains("merchant") {
        "Ah, a customer! Take a look at my wares.".to_string()
    } else if desc_lower.contains("mysterious") || desc_lower.contains("knowing") {
        "You seek answers... but are you ready to hear them?".to_string()
    } else if desc_lower.contains("guard") || desc_lower.contains("soldier") {
        "State your business, citizen.".to_string()
    } else {
        "Greetings, traveler.".to_string()
    }
}

/// Execute the 'say' tool - character speaks aloud
pub fn tool_say(call: &ToolCall) -> anyhow::Result<ToolResult> {
    let message = call.args_joined();
    if message.is_empty() {
        return Ok(ToolResult::failure("What do you want to say?".to_string()));
    }

    // The description is shown to Zulip - include the speech
    Ok(ToolResult::success(
        format!("\"{}\"", message),
        "Spoke aloud".to_string(),
    ))
}

/// Execute the 'use' tool - use a consumable item from inventory
pub fn tool_use(
    world: &mut WorldState,
    character: &SmolStr,
    call: &ToolCall,
) -> anyhow::Result<ToolResult> {
    let target = call.args_joined();
    if target.is_empty() {
        return Ok(ToolResult::failure("Use what?".to_string()));
    }

    let char_state = world
        .get_character(character)
        .ok_or_else(|| anyhow::anyhow!("Character {} not found", character))?;

    // Find matching object in inventory (immutable borrow first)
    let mut found_idx: Option<usize> = None;
    let mut found_obj_id: Option<SmolStr> = None;

    for (idx, obj_id) in char_state.inventory.iter().enumerate() {
        if let Some(obj) = world.object_defs.get(obj_id) {
            if matches_name(&obj.name, &target) {
                found_idx = Some(idx);
                found_obj_id = Some(obj_id.clone());
                break;
            }
        }
    }

    let idx = found_idx.ok_or_else(|| anyhow::anyhow!("You don't have '{}'.", target))?;
    let obj_id = found_obj_id.unwrap();

    // Get object info
    let obj = world
        .object_defs
        .get(&obj_id)
        .ok_or_else(|| anyhow::anyhow!("Object not found"))?;

    // Check if consumable
    let is_consumable = obj.tags.iter().any(|t| t.as_str() == "consumable");
    if !is_consumable {
        return Ok(ToolResult::failure(format!(
            "The {} cannot be used that way.",
            obj.name
        )));
    }

    // Determine effect based on tags
    let obj_name = obj.name.clone();
    let is_healing = obj.tags.iter().any(|t| t.as_str() == "healing");
    let is_food = obj.tags.iter().any(|t| t.as_str() == "food");
    let is_drink = obj.tags.iter().any(|t| t.as_str() == "drink");

    // Now we can mutate
    let char_state = world.get_character_mut(character).unwrap();

    // Remove from inventory
    char_state.inventory.remove(idx);

    // Apply effect
    let effect_description = if is_healing {
        let heal_amount = 25;
        let old_health = char_state.stats.health;
        char_state.stats.health =
            (char_state.stats.health + heal_amount).min(char_state.stats.max_health);
        let actual_heal = char_state.stats.health - old_health;

        if actual_heal > 0 {
            format!(
                "You drink the {}. A warm energy flows through you, restoring {} health. ({}/{})",
                obj_name, actual_heal, char_state.stats.health, char_state.stats.max_health
            )
        } else {
            format!(
                "You drink the {}. You're already at full health.",
                obj_name
            )
        }
    } else if is_food {
        format!(
            "You eat the {}. It's quite satisfying.",
            obj_name
        )
    } else if is_drink {
        format!(
            "You drink the {}. Refreshing!",
            obj_name
        )
    } else {
        format!("You use the {}.", obj_name)
    };

    Ok(ToolResult::success(
        effect_description,
        format!("Used {}", obj_name),
    ))
}

/// Execute the 'attack' tool - attack an NPC
pub fn tool_attack(
    world: &mut WorldState,
    character: &SmolStr,
    call: &ToolCall,
) -> anyhow::Result<ToolResult> {
    let target = call.args_joined();
    if target.is_empty() {
        return Ok(ToolResult::failure("Attack what?".to_string()));
    }

    // Get character info first (immutable borrow)
    let (room_id, weapon_damage, char_strength) = {
        let char_state = world
            .get_character(character)
            .ok_or_else(|| anyhow::anyhow!("Character {} not found", character))?;

        let weapon_damage = char_state
            .equipment
            .weapon
            .as_ref()
            .and_then(|id| world.object_defs.get(id))
            .and_then(|obj| obj.stat_mods.get("strength"))
            .copied()
            .unwrap_or(0);

        (
            char_state.location.clone(),
            weapon_damage,
            char_state.stats.strength,
        )
    };

    // Find NPC in room
    let room = world
        .spatial
        .rooms
        .get(&room_id)
        .ok_or_else(|| anyhow::anyhow!("Room not found"))?;

    let mut found_npc_id: Option<SmolStr> = None;

    for npc_id in &room.npcs {
        if let Some(npc) = world.npc_defs.get(npc_id) {
            if matches_name(&npc.name, &target) {
                found_npc_id = Some(npc_id.clone());
                break;
            }
        }
    }

    let npc_id = found_npc_id.ok_or_else(|| anyhow::anyhow!("'{}' is not here.", target))?;

    // Get NPC info
    let npc = world
        .npc_defs
        .get(&npc_id)
        .ok_or_else(|| anyhow::anyhow!("NPC not found"))?;

    let npc_name = npc.name.clone();
    let npc_max_health = npc.max_health;

    // Calculate damage: base (5) + weapon bonus + small strength modifier
    let base_damage = 5;
    let strength_bonus = (char_strength - 10) / 2; // +1 per 2 points over 10
    let total_damage = (base_damage + weapon_damage + strength_bonus).max(1);

    // Apply damage (need mutable borrow of npc_defs)
    let npc = world.npc_defs.get_mut(&npc_id).unwrap();
    npc.health -= total_damage;

    if npc.health <= 0 {
        // NPC defeated - remove from room
        let room = world.spatial.rooms.get_mut(&room_id).unwrap();
        room.npcs.retain(|id| id != &npc_id);

        Ok(ToolResult::success(
            format!(
                "You strike {} for {} damage! They collapse, defeated.",
                npc_name, total_damage
            ),
            format!("Defeated {}", npc_name),
        ))
    } else {
        Ok(ToolResult::success(
            format!(
                "You strike {} for {} damage! ({}/{} HP)",
                npc_name, total_damage, npc.health, npc_max_health
            ),
            format!("Attacked {} for {} damage", npc_name, total_damage),
        ))
    }
}
