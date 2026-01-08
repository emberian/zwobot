use super::definitions::{ToolCall, ToolResult};
use super::matching::matches_name;
use crate::world::WorldState;
use smol_str::SmolStr;

/// Execute the 'inventory' tool - show character's inventory
pub fn tool_inventory(world: &WorldState, character: &SmolStr) -> anyhow::Result<ToolResult> {
    let char_state = world.get_character(character)
        .ok_or_else(|| anyhow::anyhow!("Character {} not found", character))?;

    if char_state.inventory.is_empty() {
        return Ok(ToolResult::success(
            "Your inventory is empty.".to_string(),
            "Checked inventory (empty)".to_string(),
        ));
    }

    let mut description = String::new();
    description.push_str("**Inventory:**\n\n");

    for obj_id in &char_state.inventory {
        if let Some(obj) = world.object_defs.get(obj_id) {
            description.push_str(&format!("- {} ({})\n", obj.name, obj.id));
        }
    }

    // Show equipped items
    let equipped_count = [
        &char_state.equipment.weapon,
        &char_state.equipment.armor,
        &char_state.equipment.accessory,
    ]
    .iter()
    .filter(|e| e.is_some())
    .count();

    if equipped_count > 0 {
        description.push_str("\n**Equipped:**\n");

        if let Some(weapon_id) = &char_state.equipment.weapon {
            if let Some(weapon) = world.object_defs.get(weapon_id) {
                description.push_str(&format!("- Weapon: {}\n", weapon.name));
            }
        }

        if let Some(armor_id) = &char_state.equipment.armor {
            if let Some(armor) = world.object_defs.get(armor_id) {
                description.push_str(&format!("- Armor: {}\n", armor.name));
            }
        }

        if let Some(acc_id) = &char_state.equipment.accessory {
            if let Some(acc) = world.object_defs.get(acc_id) {
                description.push_str(&format!("- Accessory: {}\n", acc.name));
            }
        }
    }

    Ok(ToolResult::success(
        description,
        format!("Checked inventory ({} items)", char_state.inventory.len()),
    ))
}

/// Execute the 'take' tool - pick up an object from the room
pub fn tool_take(
    world: &mut WorldState,
    character: &SmolStr,
    call: &ToolCall,
) -> anyhow::Result<ToolResult> {
    let target = call.args_joined();
    if target.is_empty() {
        return Ok(ToolResult::failure("Take what?".to_string()));
    }

    let char_state = world.get_character(character)
        .ok_or_else(|| anyhow::anyhow!("Character {} not found", character))?;

    let room_id = char_state.location.clone();

    // Find matching object in room (immutable borrow)
    let mut found_obj_id: Option<SmolStr> = None;

    {
        let room = world.spatial.rooms.get(&room_id)
            .ok_or_else(|| anyhow::anyhow!("Room {} not found", room_id))?;

        for obj_id in &room.objects {
            if let Some(obj) = world.object_defs.get(obj_id) {
                if matches_name(&obj.name, &target) {
                    if !obj.takeable {
                        return Ok(ToolResult::failure(
                            format!("You can't take the {}.", obj.name)
                        ));
                    }
                    found_obj_id = Some(obj_id.clone());
                    break;
                }
            }
        }
    }

    let obj_id = found_obj_id.ok_or_else(|| anyhow::anyhow!("'{}' not found here.", target))?;

    // Remove from room
    let room = world.spatial.rooms.get_mut(&room_id).unwrap();
    room.remove_object(&obj_id);

    // Add to inventory
    let char_state = world.get_character_mut(character).unwrap();
    char_state.inventory.push(obj_id.clone());

    let obj_name = world.object_defs.get(&obj_id)
        .map(|o| o.name.as_str())
        .unwrap_or("item");

    Ok(ToolResult::success(
        format!("You pick up the {}.", obj_name),
        format!("Took {}", obj_name),
    ))
}

/// Execute the 'drop' tool - drop an object from inventory
pub fn tool_drop(
    world: &mut WorldState,
    character: &SmolStr,
    call: &ToolCall,
) -> anyhow::Result<ToolResult> {
    let target = call.args_joined();
    if target.is_empty() {
        return Ok(ToolResult::failure("Drop what?".to_string()));
    }

    let char_state = world.get_character(character)
        .ok_or_else(|| anyhow::anyhow!("Character {} not found", character))?;

    let room_id = char_state.location.clone();

    // Find matching object in inventory
    let mut found_obj_id: Option<SmolStr> = None;

    for obj_id in &char_state.inventory {
        if let Some(obj) = world.object_defs.get(obj_id) {
            if matches_name(&obj.name, &target) {
                found_obj_id = Some(obj_id.clone());
                break;
            }
        }
    }

    let obj_id = found_obj_id
        .ok_or_else(|| anyhow::anyhow!("You don't have '{}'.", target))?;

    // Check if item is equipped
    let char_state = world.get_character(character).unwrap();
    let is_equipped = char_state.equipment.weapon.as_ref() == Some(&obj_id)
        || char_state.equipment.armor.as_ref() == Some(&obj_id)
        || char_state.equipment.accessory.as_ref() == Some(&obj_id);

    if is_equipped {
        return Ok(ToolResult::failure(
            "You need to unequip that item first.".to_string()
        ));
    }

    // Remove from inventory
    let char_state = world.get_character_mut(character).unwrap();
    char_state.inventory.retain(|id| id != &obj_id);

    // Add to room
    let room = world.spatial.rooms.get_mut(&room_id).unwrap();
    room.add_object(obj_id.clone());

    let obj_name = world.object_defs.get(&obj_id)
        .map(|o| o.name.as_str())
        .unwrap_or("item");

    Ok(ToolResult::success(
        format!("You drop the {}.", obj_name),
        format!("Dropped {}", obj_name),
    ))
}

/// Execute the 'equip' tool - equip an item from inventory
pub fn tool_equip(
    world: &mut WorldState,
    character: &SmolStr,
    call: &ToolCall,
) -> anyhow::Result<ToolResult> {
    let target = call.args_joined();
    if target.is_empty() {
        return Ok(ToolResult::failure("Equip what?".to_string()));
    }

    let char_state = world.get_character(character)
        .ok_or_else(|| anyhow::anyhow!("Character {} not found", character))?;

    // Find matching object in inventory
    let mut found_obj_id: Option<SmolStr> = None;

    for obj_id in &char_state.inventory {
        if let Some(obj) = world.object_defs.get(obj_id) {
            if matches_name(&obj.name, &target) {
                found_obj_id = Some(obj_id.clone());
                break;
            }
        }
    }

    let obj_id = found_obj_id
        .ok_or_else(|| anyhow::anyhow!("You don't have '{}'.", target))?;

    // Get object info before mutating world
    let (obj_name, equippable, slot) = {
        let obj = world.object_defs.get(&obj_id)
            .ok_or_else(|| anyhow::anyhow!("Object not found"))?;

        if !obj.equippable {
            return Ok(ToolResult::failure(
                format!("The {} cannot be equipped.", obj.name)
            ));
        }

        let slot = obj.slot.ok_or_else(|| anyhow::anyhow!("No equipment slot defined"))?;

        (obj.name.clone(), obj.equippable, slot)
    };

    // Equip the item
    let char_state = world.get_character_mut(character).unwrap();

    use crate::world::EquipmentSlot;
    let slot_ref = match slot {
        EquipmentSlot::Weapon => &mut char_state.equipment.weapon,
        EquipmentSlot::Armor => &mut char_state.equipment.armor,
        EquipmentSlot::Accessory => &mut char_state.equipment.accessory,
    };

    let mut message = format!("You equip the {}.", obj_name);

    // Unequip previous item if any
    if let Some(prev_id) = slot_ref.replace(obj_id.clone()) {
        if let Some(prev_obj) = world.object_defs.get(&prev_id) {
            message.push_str(&format!(" (Unequipped {})", prev_obj.name));
        }
    }

    Ok(ToolResult::success(
        message,
        format!("Equipped {}", obj_name),
    ))
}
