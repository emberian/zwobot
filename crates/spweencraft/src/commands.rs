//! Commands for spweencraft bot functionality

use crate::{format_spween_display, BotEffectHandler, SpweencraftData};
use futures::future::BoxFuture;
use tulip_bot::prelude::*;

/// /spween command - start a spween scene
pub struct SpweenCommand;

impl Command<SpweencraftData> for SpweenCommand {
    fn definition(&self) -> CommandDef {
        CommandDef::new("spween", "Start a spween scene")
            .option(CommandOption::string("scene_id", "The scene ID to play").required())
    }

    fn execute<'a>(
        &'a self,
        ctx: CommandContext<'a, SpweencraftData>,
    ) -> BoxFuture<'a, Result<Response>> {
        Box::pin(async move {
            let scene_id = ctx.args.get::<String>("scene_id")?;
            let topic = ctx.topic();

            // Check if there's already an active session
            {
                let sessions = ctx.data.sessions.read().await;
                if sessions.has_session(topic) {
                    return Ok(Response::message(format!(
                        "There's already an active spween in this topic! End it first with `/spween_end`"
                    )));
                }
            }

            // Get the scene
            let scene = {
                let registry = ctx.data.registry.read().await;
                registry.get_scene(&scene_id)
            };

            let scene = match scene {
                Some(s) => s,
                None => {
                    let available = {
                        let registry = ctx.data.registry.read().await;
                        registry.all_scene_ids()
                    };

                    return Ok(Response::message(format!(
                        "Scene '{}' not found. Available scenes:\n{}",
                        scene_id,
                        available
                            .iter()
                            .map(|id| format!("- {}", id))
                            .collect::<Vec<_>>()
                            .join("\n")
                    )));
                }
            };

            // Create a new session
            let handler = BotEffectHandler::new();
            {
                let mut sessions = ctx.data.sessions.write().await;
                if let Err(e) = sessions.start_session(topic, scene.clone(), handler) {
                    return Ok(Response::message(format!("Failed to start scene: {}", e)));
                }
            }

            // Get initial display
            let (text, choices) = {
                let sessions = ctx.data.sessions.read().await;
                let session = sessions.get_session(topic).unwrap();
                let text = session.current_text();
                let choices = session.available_choices();
                (text, choices)
            };

            let display = format_spween_display(&text, &choices);

            Ok(Response::embed(
                RichEmbed::builder()
                    .title(format!("🎭 {}", scene.meta.title))
                    .description(&display)
                    .footer("Reply with a choice number (1, 2, etc.)")
                    .color(0xe74c3c)
                    .build(),
            ))
        })
    }
}

/// /spween_end command - end the current spween session
pub struct SpweenEndCommand;

impl Command<SpweencraftData> for SpweenEndCommand {
    fn definition(&self) -> CommandDef {
        CommandDef::new("spween_end", "End the current spween session")
    }

    fn execute<'a>(
        &'a self,
        ctx: CommandContext<'a, SpweencraftData>,
    ) -> BoxFuture<'a, Result<Response>> {
        Box::pin(async move {
            let topic = ctx.topic();

            let removed = {
                let mut sessions = ctx.data.sessions.write().await;
                sessions.remove_session(topic).is_some()
            };

            if removed {
                Ok(Response::message("Spween session ended."))
            } else {
                Ok(Response::message("No active spween session in this topic."))
            }
        })
    }
}

/// /spween_list command - list available scenes
pub struct SpweenListCommand;

impl Command<SpweencraftData> for SpweenListCommand {
    fn definition(&self) -> CommandDef {
        CommandDef::new("spween_list", "List all available spween scenes")
    }

    fn execute<'a>(
        &'a self,
        ctx: CommandContext<'a, SpweencraftData>,
    ) -> BoxFuture<'a, Result<Response>> {
        Box::pin(async move {
            let registry = ctx.data.registry.read().await;
            let scenes = registry.all_scene_ids();

            if scenes.is_empty() {
                return Ok(Response::message(
                    "No scenes registered yet. Add scene files to registry channels!",
                ));
            }

            let list = scenes
                .iter()
                .map(|id| format!("- `{}`", id))
                .collect::<Vec<_>>()
                .join("\n");

            Ok(Response::embed(
                RichEmbed::builder()
                    .title("🎭 Available Scenes")
                    .description(&format!("Total: {}\n\n{}", scenes.len(), list))
                    .color(0x3498db)
                    .build(),
            ))
        })
    }
}

/// /spween_reload command - reload scenes from registry channels
pub struct SpweenReloadCommand;

impl Command<SpweencraftData> for SpweenReloadCommand {
    fn definition(&self) -> CommandDef {
        CommandDef::new("spween_reload", "Reload scenes from registry channels")
    }

    fn execute<'a>(
        &'a self,
        ctx: CommandContext<'a, SpweencraftData>,
    ) -> BoxFuture<'a, Result<Response>> {
        Box::pin(async move {
            // This would need to scan registry channels and reload
            // For now, just report current status
            let registry = ctx.data.registry.read().await;
            let count = registry.scene_count();

            Ok(Response::message(format!(
                "Currently {} scenes registered. (Full reload not yet implemented)",
                count
            )))
        })
    }
}

/// Handle regular messages for spween choice selection
pub async fn handle_spween_message(
    ctx: MessageContext<'_, SpweencraftData>,
) -> Result<Option<Response>> {
    let topic = ctx.topic();
    let content = ctx.content().trim();

    // Check if there's an active session
    let has_session = {
        let sessions = ctx.data.sessions.read().await;
        sessions.has_session(topic)
    };

    if !has_session {
        // Check if this is a registry stream and the message contains ```spween blocks
        if ctx.data.is_registry_stream(ctx.channel()) {
            let blocks = crate::extract_spween_blocks(content);
            if !blocks.is_empty() {
                return handle_scene_registration(&ctx, &blocks[..]).await;
            }
        }

        return Ok(None);
    }

    // Try to parse as a choice number
    let choice_num = match content.parse::<usize>() {
        Ok(n) if n > 0 => n - 1, // Convert 1-indexed to 0-indexed
        _ => return Ok(None),    // Not a number, ignore
    };

    // Add working reaction
    ctx.react("working").await.ok();

    // Select the choice
    let result = {
        let mut sessions = ctx.data.sessions.write().await;
        let session = sessions.get_session_mut(topic).unwrap();

        // Check if choice is valid
        let available = session.available_choices();
        if choice_num >= available.len() {
            Some(Response::message(format!(
                "Invalid choice {}. Please choose 1-{}",
                choice_num + 1,
                available.len()
            )))
        } else {
            // Select the choice
            if let Err(e) = session.select_choice(choice_num) {
                Some(Response::message(format!("Error: {}", e)))
            } else {
                // Get new state
                if session.is_ended() {
                    // Session ended
                    None
                } else {
                    // Continue with new passage
                    let text = session.current_text();
                    let choices = session.available_choices();
                    let display = format_spween_display(&text, &choices);

                    Some(Response::embed(
                        RichEmbed::builder()
                            .description(&display)
                            .footer("Reply with a choice number")
                            .color(0xe74c3c)
                            .build(),
                    ))
                }
            }
        }
    };

    // Clean up if ended
    if result.is_none() {
        let mut sessions = ctx.data.sessions.write().await;
        sessions.remove_session(topic);

        ctx.unreact("working").await.ok();
        ctx.react("checkered_flag").await.ok();

        return Ok(Some(Response::embed(
            RichEmbed::builder()
                .title("The End")
                .description("The spween has concluded. Start a new one with `/spween <scene_id>`")
                .color(0x95a5a6)
                .build(),
        )));
    }

    ctx.unreact("working").await.ok();
    ctx.react("arrow_forward").await.ok();

    Ok(result)
}

/// Handle registration of spween scenes from a registry stream
async fn handle_scene_registration(
    ctx: &MessageContext<'_, SpweencraftData>,
    blocks: &[String],
) -> Result<Option<Response>> {
    let stream = ctx.channel();
    let mut registered = Vec::new();
    let mut errors = Vec::new();

    for (i, block) in blocks.iter().enumerate() {
        let filename = format!("{}/message_{}_block_{}.scene", stream, ctx.message.id, i);

        let result = {
            let mut registry = ctx.data.registry.write().await;
            registry.register_scene(block, stream, &filename)
        };

        match result {
            Ok(scene_id) => registered.push(scene_id.to_string()),
            Err(e) => errors.push((i + 1, e.to_string())),
        }
    }

    if !registered.is_empty() {
        ctx.react("white_check_mark").await.ok();
    }
    if !errors.is_empty() {
        ctx.react("warning").await.ok();
    }

    let mut response = String::new();

    if !registered.is_empty() {
        response.push_str(&format!(
            "✓ Registered {} scene(s):\n{}\n",
            registered.len(),
            registered
                .iter()
                .map(|id| format!("  - `{}`", id))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    if !errors.is_empty() {
        response.push_str(&format!(
            "\n❌ Failed to parse {} block(s):\n{}",
            errors.len(),
            errors
                .iter()
                .map(|(i, e)| format!("  - Block {}: {}", i, e))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    Ok(Some(Response::message(response)))
}
