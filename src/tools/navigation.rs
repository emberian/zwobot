use super::definitions::{ToolCall, ToolResult};
use super::matching::matches_name;
use crate::world::{Condition, WorldState};
use smol_str::SmolStr;

/// Execute the 'look' tool - describe current room
pub fn tool_look(world: &WorldState, character: &SmolStr) -> anyhow::Result<ToolResult> {
    let char_state = world.get_character(character)
        .ok_or_else(|| anyhow::anyhow!("Character {} not found", character))?;

    let room = world.spatial.rooms.get(&char_state.location)
        .ok_or_else(|| anyhow::anyhow!("Room {} not found", char_state.location))?;

    let mut description = String::new();
    description.push_str(&format!("**{}**\n\n", room.name));
    description.push_str(&format!("{}\n\n", room.description));

    // List exits
    if !room.exits.is_empty() {
        description.push_str("**Exits:** ");
        let exit_list: Vec<String> = room.exits.keys()
            .map(|dir| dir.to_string())
            .collect();
        description.push_str(&exit_list.join(", "));
        description.push_str("\n\n");
    }

    // List objects
    if !room.objects.is_empty() {
        description.push_str("**Items here:** ");
        let obj_names: Vec<String> = room.objects.iter()
            .filter_map(|id| world.object_defs.get(id))
            .map(|obj| obj.name.to_string())
            .collect();
        description.push_str(&obj_names.join(", "));
        description.push_str("\n\n");
    }

    // List NPCs
    if !room.npcs.is_empty() {
        description.push_str("**NPCs here:** ");
        let npc_names: Vec<String> = room.npcs.iter()
            .filter_map(|id| world.npc_defs.get(id))
            .map(|npc| npc.name.to_string())
            .collect();
        description.push_str(&npc_names.join(", "));
        description.push_str("\n\n");
    }

    // List other characters
    let other_chars: Vec<&SmolStr> = room.characters.iter()
        .filter(|c| *c != character)
        .collect();

    if !other_chars.is_empty() {
        description.push_str("**Also here:** ");
        description.push_str(&other_chars.iter()
            .map(|c| c.as_str())
            .collect::<Vec<_>>()
            .join(", "));
        description.push_str("\n");
    }

    Ok(ToolResult::success(
        description,
        format!("Looked around {}", room.name),
    ))
}

/// Execute the 'examine' tool - describe an object or NPC
pub fn tool_examine(
    world: &WorldState,
    character: &SmolStr,
    call: &ToolCall,
) -> anyhow::Result<ToolResult> {
    let target = call.args_joined();
    if target.is_empty() {
        return Ok(ToolResult::failure("Examine what?".to_string()));
    }

    let char_state = world.get_character(character)
        .ok_or_else(|| anyhow::anyhow!("Character {} not found", character))?;

    let room = world.spatial.rooms.get(&char_state.location)
        .ok_or_else(|| anyhow::anyhow!("Room {} not found", char_state.location))?;

    // Check inventory first
    for obj_id in &char_state.inventory {
        if let Some(obj) = world.object_defs.get(obj_id) {
            if matches_name(&obj.name, &target) {
                let mut desc = format!("**{}**\n\n{}", obj.name, obj.description);

                if !obj.tags.is_empty() {
                    desc.push_str(&format!("\n\n*Tags: {}*", obj.tags.join(", ")));
                }

                if obj.equippable {
                    desc.push_str(&format!("\n\n*Can be equipped in {:?} slot*", obj.slot));
                }

                return Ok(ToolResult::success(
                    desc,
                    format!("Examined {}", obj.name),
                ));
            }
        }
    }

    // Check room objects
    for obj_id in &room.objects {
        if let Some(obj) = world.object_defs.get(obj_id) {
            if matches_name(&obj.name, &target) {
                let mut desc = format!("**{}**\n\n{}", obj.name, obj.description);

                if !obj.tags.is_empty() {
                    desc.push_str(&format!("\n\n*Tags: {}*", obj.tags.join(", ")));
                }

                return Ok(ToolResult::success(
                    desc,
                    format!("Examined {}", obj.name),
                ));
            }
        }
    }

    // Check NPCs
    for npc_id in &room.npcs {
        if let Some(npc) = world.npc_defs.get(npc_id) {
            if matches_name(&npc.name, &target) {
                let desc = format!(
                    "**{}**\n\n{}\n\n*Health: {}/{}*",
                    npc.name, npc.description, npc.health, npc.max_health
                );

                return Ok(ToolResult::success(
                    desc,
                    format!("Examined {}", npc.name),
                ));
            }
        }
    }

    Ok(ToolResult::failure(format!("You don't see '{}' here.", target)))
}

/// Execute the 'go' tool - move to another room
pub fn tool_go(
    world: &mut WorldState,
    character: &SmolStr,
    call: &ToolCall,
) -> anyhow::Result<ToolResult> {
    let direction = call.first_arg()
        .ok_or_else(|| anyhow::anyhow!("Go where? Specify a direction."))?;

    let char_state = world.get_character(character)
        .ok_or_else(|| anyhow::anyhow!("Character {} not found", character))?;

    let current_room_id = char_state.location.clone();
    let current_room = world.spatial.rooms.get(&current_room_id)
        .ok_or_else(|| anyhow::anyhow!("Room {} not found", current_room_id))?;

    // Find exit
    let exit = current_room.exits.get(direction.as_str())
        .ok_or_else(|| anyhow::anyhow!("There's no exit in that direction."))?;

    // Check condition if any
    if let Some(condition) = &exit.requires {
        if !check_condition(world, character, condition) {
            return Ok(ToolResult::failure(
                "The way is blocked. You need something to proceed.".to_string()
            ));
        }
    }

    let new_room_id = exit.target_room.clone();

    // Move character
    world.move_character(character, new_room_id.clone())?;

    let new_room = world.spatial.rooms.get(&new_room_id)
        .ok_or_else(|| anyhow::anyhow!("Target room {} not found", new_room_id))?;

    // Return description of new room
    let mut description = String::new();
    description.push_str(&format!("You move {}.\n\n", direction));
    description.push_str(&format!("**{}**\n\n", new_room.name));
    description.push_str(&new_room.description);

    Ok(ToolResult::success(
        description,
        format!("Moved {} to {}", direction, new_room.name),
    ))
}

/// Check if a condition is met
fn check_condition(world: &WorldState, character: &SmolStr, condition: &Condition) -> bool {
    match condition {
        Condition::HasItem(item_id) => {
            if let Some(char_state) = world.get_character(character) {
                char_state.inventory.contains(item_id)
            } else {
                false
            }
        }
        Condition::Variable { name, value } => {
            world.variables.get(name) == Some(value)
        }
        Condition::And(conditions) => {
            conditions.iter().all(|c| check_condition(world, character, c))
        }
        Condition::Or(conditions) => {
            conditions.iter().any(|c| check_condition(world, character, c))
        }
    }
}
