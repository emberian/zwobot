use super::definitions::{ToolCall, ToolResult};
use super::interaction::{tool_attack, tool_say, tool_talk, tool_use};
use super::inventory::{tool_drop, tool_equip, tool_inventory, tool_take};
use super::navigation::{tool_examine, tool_go, tool_look};
use super::scenes::tool_choose;
use crate::scenes::SceneManager;
use crate::world::WorldState;
use smol_str::SmolStr;

/// Execute a tool call against the world state
pub fn execute_tool(
    world: &mut WorldState,
    character: &SmolStr,
    call: &ToolCall,
    scene_manager: Option<&mut SceneManager>,
) -> anyhow::Result<ToolResult> {
    match call.tool_name.as_str() {
        // Navigation tools (read-only, take immutable ref)
        "look" | "l" => tool_look(world, character),
        "examine" | "ex" | "x" => tool_examine(world, character, call),

        // Movement tool (mutates world)
        "go" | "move" => tool_go(world, character, call),

        // Inventory tools
        "inventory" | "inv" | "i" => tool_inventory(world, character),
        "take" | "get" | "grab" => tool_take(world, character, call),
        "drop" => tool_drop(world, character, call),
        "equip" | "wear" | "wield" => tool_equip(world, character, call),

        // Interaction tools
        "talk" | "speak" => tool_talk(world, character, call, scene_manager),
        "say" => tool_say(call),
        "use" | "consume" | "drink" | "eat" => tool_use(world, character, call),
        "attack" | "hit" | "fight" => tool_attack(world, character, call),

        // Scene tools
        "choose" | "select" => {
            if let Some(manager) = scene_manager {
                tool_choose(world, character, call, manager)
            } else {
                Ok(ToolResult::failure("Scene system not available.".to_string()))
            }
        }

        // Unknown tool
        _ => Ok(ToolResult::failure(format!(
            "Unknown tool: '{}'. Try: look, examine, go, inventory, take, drop, equip, talk, say, use, attack, choose",
            call.tool_name
        ))),
    }
}

/// Get a list of available tools for prompting
pub fn get_available_tools(world: &WorldState, character: &SmolStr) -> String {
    let char_state = match world.get_character(character) {
        Some(cs) => cs,
        None => return String::new(),
    };

    let room = match world.spatial.rooms.get(&char_state.location) {
        Some(r) => r,
        None => return String::new(),
    };

    let mut tools = String::new();

    // Check if in active scene - if so, show scene-specific tools
    if char_state.active_scene.is_some() {
        tools.push_str("**You are in a conversation.**\n\n");
        tools.push_str("- `<tool>choose N</tool>` - Select choice number N\n");
        tools.push_str("- `<tool>say MESSAGE</tool>` - Say something aloud\n");
        tools.push_str("\nReview the choices above and use `choose N` to respond.");
        return tools;
    }

    tools.push_str("**Available Tools:**\n\n");

    // Always available
    tools.push_str("- `<tool>look</tool>` - Look around the current room\n");
    tools.push_str("- `<tool>inventory</tool>` - Check your inventory\n");

    // Movement
    if !room.exits.is_empty() {
        tools.push_str("- `<tool>go DIRECTION</tool>` - Move in a direction. ");
        tools.push_str("Available exits: ");
        let exits: Vec<String> = room.exits.keys().map(|s| s.to_string()).collect();
        tools.push_str(&exits.join(", "));
        tools.push_str("\n");
    }

    // Examine
    let examinable_count = room.objects.len() + room.npcs.len() + char_state.inventory.len();
    if examinable_count > 0 {
        tools.push_str("- `<tool>examine THING</tool>` - Examine an object or NPC\n");
    }

    // Take
    if !room.objects.is_empty() {
        tools.push_str("- `<tool>take ITEM</tool>` - Pick up an item. ");
        tools.push_str("Available items: ");
        let items: Vec<String> = room
            .objects
            .iter()
            .filter_map(|id| world.object_defs.get(id))
            .filter(|obj| obj.takeable)
            .map(|obj| obj.name.to_string())
            .collect();
        if !items.is_empty() {
            tools.push_str(&items.join(", "));
        }
        tools.push_str("\n");
    }

    // Drop/Equip
    if !char_state.inventory.is_empty() {
        tools.push_str("- `<tool>drop ITEM</tool>` - Drop an item from inventory\n");

        let has_equippable = char_state.inventory.iter().any(|id| {
            world
                .object_defs
                .get(id)
                .map(|obj| obj.equippable)
                .unwrap_or(false)
        });

        if has_equippable {
            tools.push_str("- `<tool>equip ITEM</tool>` - Equip an item from inventory\n");
        }

        // Use consumables
        let has_consumable = char_state.inventory.iter().any(|id| {
            world
                .object_defs
                .get(id)
                .map(|obj| obj.tags.iter().any(|t| t.as_str() == "consumable"))
                .unwrap_or(false)
        });

        if has_consumable {
            tools.push_str("- `<tool>use ITEM</tool>` - Use a consumable item\n");
        }
    }

    // Talk to NPCs
    if !room.npcs.is_empty() {
        let npc_names: Vec<String> = room
            .npcs
            .iter()
            .filter_map(|id| world.npc_defs.get(id))
            .map(|npc| npc.name.to_string())
            .collect();

        tools.push_str("- `<tool>talk NPC</tool>` - Talk to someone. ");
        tools.push_str("NPCs here: ");
        tools.push_str(&npc_names.join(", "));
        tools.push_str("\n");

        tools.push_str("- `<tool>attack TARGET</tool>` - Attack someone\n");
    }

    // Say something
    tools.push_str("- `<tool>say MESSAGE</tool>` - Say something aloud\n");

    tools.push_str("\nUse ONLY ONE tool per response.");

    tools
}
