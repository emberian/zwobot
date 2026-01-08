//! Scene-related tools (choose).

use super::definitions::{ToolCall, ToolResult};
use crate::scenes::{continue_scene, format_scene_for_zulip, SceneManager};
use crate::world::WorldState;
use smol_str::SmolStr;

/// Execute the choose tool - select a choice in an active scene.
pub fn tool_choose(
    world: &mut WorldState,
    character: &SmolStr,
    call: &ToolCall,
    scene_manager: &mut SceneManager,
) -> anyhow::Result<ToolResult> {
    // Get choice index from args
    let choice_idx: usize = call
        .args
        .first()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| anyhow::anyhow!("Usage: choose <number>"))?;

    // Check if character is in an active scene
    let active_scene = world
        .get_character(character)
        .and_then(|cs| cs.active_scene.as_ref())
        .map(|as_| as_.scene_id.clone());

    let scene_id = match active_scene {
        Some(id) => id,
        None => {
            return Ok(ToolResult::failure(
                "You're not in a conversation. Use `talk` to speak with someone.".to_string(),
            ));
        }
    };

    // Load scene
    let scene = scene_manager.load_scene(&scene_id)?;

    // Continue scene with choice
    let output = continue_scene(world, character, scene, choice_idx)?;

    let description = format_scene_for_zulip(&output);
    let summary = output.summary;

    Ok(ToolResult::success(description, summary))
}
