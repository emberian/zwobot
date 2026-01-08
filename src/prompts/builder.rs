use crate::tools::get_available_tools;
use crate::world::WorldState;
use smol_str::SmolStr;
use tracing::trace;

/// Build a prompt for the LLM to interpret a player's command
/// This will be fully rewritten in Phase 2 - for now it's a minimal version
pub fn build_interpreter_prompt(
    world: &WorldState,
    player_name: &str,
    player_message: &str,
) -> String {
    trace!("Building interpreter prompt for player {}", player_name);

    let mut prompt = String::new();

    prompt.push_str("You are the game master for this interactive fiction.\n\n");

    // Player state
    if let Some(char_state) = world.get_character(player_name) {
        // Current location
        if let Some(room) = world.spatial.rooms.get(&char_state.location) {
            prompt.push_str(&format!("**Current Location:** {}\n", room.name));
            prompt.push_str(&format!("{}\n\n", room.description));

            // Items here
            if !room.objects.is_empty() {
                prompt.push_str("**Items here:** ");
                let items: Vec<String> = room
                    .objects
                    .iter()
                    .filter_map(|id| world.object_defs.get(id))
                    .map(|obj| obj.name.to_string())
                    .collect();
                prompt.push_str(&items.join(", "));
                prompt.push_str("\n");
            }

            // NPCs here
            if !room.npcs.is_empty() {
                prompt.push_str("**NPCs here:** ");
                let npcs: Vec<String> = room
                    .npcs
                    .iter()
                    .filter_map(|id| world.npc_defs.get(id))
                    .map(|npc| format!("{} - {}", npc.name, npc.description))
                    .collect();
                prompt.push_str(&npcs.join("; "));
                prompt.push_str("\n");
            }

            // Exits
            if !room.exits.is_empty() {
                prompt.push_str("**Exits:** ");
                let exits: Vec<String> = room.exits.keys().map(|s| s.to_string()).collect();
                prompt.push_str(&exits.join(", "));
                prompt.push_str("\n");
            }

            prompt.push_str("\n");
        }

        // Player inventory
        prompt.push_str(&format!(
            "**Player's inventory:** {}\n",
            if char_state.inventory.is_empty() {
                "empty".to_string()
            } else {
                char_state
                    .inventory
                    .iter()
                    .filter_map(|id| world.object_defs.get(id))
                    .map(|obj| obj.name.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        ));

        prompt.push_str(&format!(
            "**Player's health:** {}/{}\n\n",
            char_state.stats.health, char_state.stats.max_health
        ));
    }

    // Available actions
    let player_smol: SmolStr = player_name.into();
    let tools_list = get_available_tools(world, &player_smol);
    prompt.push_str("---\n\n");
    prompt.push_str(&tools_list);
    prompt.push_str("\n\n");

    // Player's message
    prompt.push_str("---\n\n");
    prompt.push_str(&format!("**The player says:** \"{}\"\n\n", player_message));

    // Instructions
    prompt.push_str("Interpret what the player wants to do. Respond with:\n");
    prompt.push_str("1. The action tag: <action>command args</action>\n");
    prompt.push_str("2. Your narrative description of what happens\n\n");
    prompt.push_str("Be atmospheric and immersive. Bring the world to life.\n");
    prompt.push_str("If the player is talking to an NPC, voice that NPC based on their description.\n");

    trace!("Interpreter prompt built: {} chars", prompt.len());

    prompt
}

/// Extract the action from LLM output
pub fn extract_action(llm_output: &str) -> Option<String> {
    let start = llm_output.find("<action>")?;
    let end = llm_output.find("</action>")?;
    if start < end {
        Some(llm_output[start + 8..end].trim().to_string())
    } else {
        None
    }
}

/// Extract the narrative from LLM output (everything outside the action tag)
pub fn extract_narrative(llm_output: &str) -> String {
    // Remove the action tag and return everything else
    if let (Some(start), Some(end)) = (llm_output.find("<action>"), llm_output.find("</action>")) {
        let before = &llm_output[..start];
        let after = &llm_output[end + 9..];
        format!("{}{}", before.trim(), after.trim()).trim().to_string()
    } else {
        llm_output.trim().to_string()
    }
}
