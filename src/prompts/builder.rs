use crate::config::Character;
use crate::scenes::{format_scene_for_prompt, SceneManager, SceneOutput};
use crate::tools::get_available_tools;
use crate::turn::TurnCoordinator;
use crate::world::WorldState;
use crate::zulip::Message;
use smol_str::SmolStr;
use tracing::trace;

/// Build a prompt for a character's turn
pub fn build_turn_prompt(
    world: &WorldState,
    character: &Character,
    history: &[Message],
    bot_id: i64,
    coordinator: Option<&TurnCoordinator>,
    scene_context: Option<&SceneOutput>,
) -> String {
    trace!("Building prompt for {} (in_scene={})", character.name, scene_context.is_some());

    let mut prompt = String::new();

    // 1. Character personality
    if !character.system_prompt.is_empty() {
        prompt.push_str(&character.system_prompt);
        prompt.push_str("\n\n");
    }

    prompt.push_str(&format!("You are {}.\n\n", character.name));

    // 2. Character state
    if let Some(char_state) = world.get_character(&character.name) {
        prompt.push_str("**Your Current State:**\n");
        prompt.push_str(&format!(
            "- Health: {}/{}\n",
            char_state.stats.health, char_state.stats.max_health
        ));

        if !char_state.inventory.is_empty() {
            prompt.push_str("- Carrying: ");
            let items: Vec<String> = char_state
                .inventory
                .iter()
                .filter_map(|id| world.object_defs.get(id))
                .map(|obj| obj.name.to_string())
                .collect();
            prompt.push_str(&items.join(", "));
            prompt.push_str("\n");
        }

        // Show equipped items
        let mut equipped = Vec::new();
        if let Some(weapon_id) = &char_state.equipment.weapon {
            if let Some(weapon) = world.object_defs.get(weapon_id) {
                equipped.push(format!("Weapon: {}", weapon.name));
            }
        }
        if let Some(armor_id) = &char_state.equipment.armor {
            if let Some(armor) = world.object_defs.get(armor_id) {
                equipped.push(format!("Armor: {}", armor.name));
            }
        }
        if !equipped.is_empty() {
            prompt.push_str("- Equipped: ");
            prompt.push_str(&equipped.join(", "));
            prompt.push_str("\n");
        }

        prompt.push_str("\n");

        // 3. Active scene context (if in a scene)
        if let Some(scene_output) = scene_context {
            prompt.push_str(&format_scene_for_prompt(scene_output));
            prompt.push_str("\n\n");
        } else {
            // 3b. Current room description (only if not in scene)
            if let Some(room) = world.spatial.rooms.get(&char_state.location) {
                prompt.push_str("**Current Location:**\n");
                prompt.push_str(&format!("{}\n\n", room.name));
                prompt.push_str(&format!("{}\n\n", room.description));

                if !room.exits.is_empty() {
                    prompt.push_str("Exits: ");
                    let exits: Vec<String> = room.exits.keys().map(|s| s.to_string()).collect();
                    prompt.push_str(&exits.join(", "));
                    prompt.push_str("\n\n");
                }

                if !room.objects.is_empty() {
                    prompt.push_str("Items here: ");
                    let items: Vec<String> = room
                        .objects
                        .iter()
                        .filter_map(|id| world.object_defs.get(id))
                        .map(|obj| obj.name.to_string())
                        .collect();
                    prompt.push_str(&items.join(", "));
                    prompt.push_str("\n\n");
                }

                if !room.npcs.is_empty() {
                    prompt.push_str("NPCs here: ");
                    let npcs: Vec<String> = room
                        .npcs
                        .iter()
                        .filter_map(|id| world.npc_defs.get(id))
                        .map(|npc| npc.name.to_string())
                        .collect();
                    prompt.push_str(&npcs.join(", "));
                    prompt.push_str("\n\n");
                }

                // Show other characters
                let other_chars: Vec<&SmolStr> = room
                    .characters
                    .iter()
                    .filter(|c| c.as_str() != character.name)
                    .collect();

                if !other_chars.is_empty() {
                    prompt.push_str("Also here: ");
                    prompt.push_str(
                        &other_chars
                            .iter()
                            .map(|c| c.as_str())
                            .collect::<Vec<_>>()
                            .join(", "),
                    );
                    prompt.push_str("\n\n");
                }
            }
        }
    }

    // 4. Recent actions by other characters
    if let Some(coord) = coordinator {
        if let Some(actions_str) = coord.format_recent_actions_for(&character.name) {
            prompt.push_str("**Recent Actions by Others:**\n");
            prompt.push_str(&actions_str);
            prompt.push_str("\n\n");
        }
    }

    // 5. Conversation history (recent messages)
    if !history.is_empty() {
        prompt.push_str("**Recent Conversation:**\n");

        // Show last 5 messages for context
        let recent_count = history.len().min(5);
        let recent_messages = &history[history.len() - recent_count..];

        for msg in recent_messages {
            let speaker = if msg.sender_id == bot_id {
                // This is from the bot - could be this character or another
                // For now, just show as the character name
                &character.name
            } else {
                &msg.sender_full_name
            };

            prompt.push_str(&format!("{}: {}\n", speaker, msg.content));
        }

        prompt.push_str("\n");
    }

    // 6. Available tools
    let tools_list = get_available_tools(world, &character.name.clone().into());
    prompt.push_str(&tools_list);
    prompt.push_str("\n\n");

    // 7. Instructions
    prompt.push_str("**Instructions:**\n");
    if scene_context.is_some() {
        prompt.push_str("1. Consider how you want to respond in this conversation\n");
        prompt.push_str("2. Use `choose N` to select your response\n");
        prompt.push_str("3. Respond with your thinking, then use the tool\n\n");
        prompt.push_str("Format: Write your thoughts, then <tool>choose N</tool>\n\n");
        prompt.push_str("Example: \"I want to ask about the quest. <tool>choose 1</tool>\"\n\n");
    } else {
        prompt.push_str("1. Think about what you want to do based on the situation\n");
        prompt.push_str("2. Choose ONE tool to use\n");
        prompt.push_str("3. Respond with your thinking, then use the tool\n\n");
        prompt.push_str("Format: Write your thoughts, then <tool>toolname args</tool>\n\n");
        prompt.push_str("Example: \"I should explore the area. <tool>look</tool>\"\n\n");
    }
    prompt.push_str("Your turn:");

    trace!("Prompt built: {} chars total", prompt.len());

    prompt
}

/// Get scene output for a character if they're in an active scene.
pub fn get_active_scene_output(
    world: &WorldState,
    character: &str,
    scene_manager: &mut SceneManager,
) -> Option<SceneOutput> {
    let char_state = world.get_character(character)?;
    let active_scene = char_state.active_scene.as_ref()?;

    // Load scene and get current state
    let scene = scene_manager.load_scene(&active_scene.scene_id).ok()?;

    // Create a temporary runtime to get current state without modifying world
    // We need to reconstruct the scene state from the stored passage
    use crate::scenes::SceneContext;
    use spween::Runtime;

    // Create a dummy world clone just to read scene state
    // This is a bit wasteful but ensures we don't modify state
    let mut world_clone = world.clone();
    let context = SceneContext::new(&mut world_clone, character.into());
    let mut runtime = Runtime::new(scene, context).ok()?;

    // Jump to current passage if not at intro
    if active_scene.current_passage.as_str() != "intro" {
        runtime.jump_to(&active_scene.current_passage).ok()?;
    }

    let prose = runtime.current_prose().unwrap_or_default();
    let choices: Vec<(usize, String, bool)> = runtime
        .current_choices()
        .into_iter()
        .map(|c| (c.index, c.text.to_string(), c.available))
        .collect();
    let ended = runtime.is_ended();

    Some(SceneOutput {
        prose,
        choices,
        ended,
        summary: String::new(),
    })
}
