//! Zwobot - Debate club bot using Tulip framework
//!
//! A bot that engages users in structured debates using local LLMs.

use debate::{build_judge_prompt, build_opponent_prompt, DebateManager};
use llm::{ImageEngine, LlmEngine};
use rhai_games::RhaiGamesData;
use serde::Deserialize;
use spweencraft::SpweencraftData;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tulip_bot::prelude::*;

// Include bundled spween scenes
const LONG_SPWEEN: &str = include_str!("../../../long.spween");

// Use anyhow for main, tulip_bot::Result for handlers

/// Bot configuration
#[derive(Debug, Deserialize)]
struct AppConfig {
    channel: String,
    debate_opponent: ModelConfig,
    debate_judge: ModelConfig,
    image_model: ImageModelConfig,
}

#[derive(Debug, Deserialize, Clone)]
struct ModelConfig {
    model_id: String,
    quantization: String,
    #[serde(default = "default_max_tokens")]
    max_tokens: u32,
    #[serde(default = "default_temperature")]
    temperature: f32,
}

fn default_max_tokens() -> u32 {
    1024
}

fn default_temperature() -> f32 {
    0.7
}

#[derive(Debug, Deserialize, Clone)]
struct ImageModelConfig {
    model_id: String,
    #[serde(default)]
    offloaded: bool,
    #[serde(default = "default_image_width")]
    default_width: usize,
    #[serde(default = "default_image_height")]
    default_height: usize,
}

fn default_image_width() -> usize {
    1280
}

fn default_image_height() -> usize {
    720
}

impl AppConfig {
    fn load() -> anyhow::Result<Self> {
        let config_str = std::fs::read_to_string("config/default.toml")?;
        let config: AppConfig = toml::from_str(&config_str)?;
        Ok(config)
    }
}

/// Shared bot data
struct BotData {
    debates: Arc<RwLock<DebateManager>>,
    opponent_model: Arc<RwLock<Option<Arc<LlmEngine>>>>,
    judge_model: Arc<RwLock<Option<Arc<LlmEngine>>>>,
    image_model: Arc<RwLock<Option<Arc<ImageEngine>>>>,
    opponent_config: ModelConfig,
    judge_config: ModelConfig,
    image_config: ImageModelConfig,
    spweencraft: Arc<SpweencraftData>,
    rhai_games: Arc<RhaiGamesData>,
}

/// /debate command - start a new debate
struct DebateCommand;

impl Command<BotData> for DebateCommand {
    fn definition(&self) -> CommandDef {
        CommandDef::new("debate", "Start a new debate with a proposition")
            .option(CommandOption::string("proposition", "The proposition to debate").required())
    }

    fn execute<'a>(&'a self, ctx: CommandContext<'a, BotData>) -> BoxFuture<'a, tulip_bot::Result<Response>> {
        Box::pin(async move {
            let proposition = ctx.args.get::<String>("proposition")?;
            let topic = ctx.topic();
            let sender = ctx.sender_name();

            // Start the debate
            {
                let mut debates = ctx.data.debates.write().await;
                debates.start_debate(topic, &proposition);
            }

            let response = Response::embed(
                RichEmbed::builder()
                    .title("New Debate Started!")
                    .description(&proposition)
                    .field("FOR", sender, true)
                    .field("AGAINST", "Bot", true)
                    .footer("Make your opening argument! Say 'judge' when ready for judgment.")
                    .color(0x3498db)
                    .build(),
            );

            Ok(response)
        })
    }
}

/// /judge command - request a judgment
struct JudgeCommand;

impl Command<BotData> for JudgeCommand {
    fn definition(&self) -> CommandDef {
        CommandDef::new("judge", "Request a judgment on the current debate")
    }

    fn execute<'a>(&'a self, ctx: CommandContext<'a, BotData>) -> BoxFuture<'a, tulip_bot::Result<Response>> {
        Box::pin(async move {
            let topic = ctx.topic();

            // Get debate state
            let debate = {
                let debates = ctx.data.debates.read().await;
                debates.get(topic).cloned()
            };

            let debate = match debate {
                Some(d) if d.has_arguments() => d,
                Some(_) => {
                    return Ok(Response::message("The debate has no arguments yet!"));
                }
                None => {
                    return Ok(Response::message(
                        "No active debate. Start one with `/debate <proposition>`",
                    ));
                }
            };

            // Add working reaction
            ctx.react("working").await.ok();

            // Ensure judge model is loaded
            let judge = get_or_load_model(
                ctx.data.judge_model.clone(),
                &ctx.data.judge_config,
            )
            .await
            .map_err(|e| tulip_bot::TulipError::Command(format!("Model load failed: {}", e)))?;

            // Generate judgment
            let prompt = build_judge_prompt(&debate);
            let judgment = judge.generate(&prompt).await.map_err(|e| {
                tulip_bot::TulipError::Command(format!("Judge generation failed: {}", e))
            })?;

            // Clear the debate
            {
                let mut debates = ctx.data.debates.write().await;
                debates.clear(topic);
            }

            ctx.unreact("working").await.ok();
            ctx.react("scales").await.ok();

            let response = Response::embed(
                RichEmbed::builder()
                    .title("The Judge's Verdict")
                    .description(&judgment)
                    .footer("Debate concluded. Start a new one with /debate")
                    .color(0x9b59b6)
                    .build(),
            );

            Ok(response)
        })
    }
}

/// /clear command - clear the debate
struct ClearCommand;

impl Command<BotData> for ClearCommand {
    fn definition(&self) -> CommandDef {
        CommandDef::new("clear", "Clear the current debate")
    }

    fn execute<'a>(&'a self, ctx: CommandContext<'a, BotData>) -> BoxFuture<'a, tulip_bot::Result<Response>> {
        Box::pin(async move {
            let topic = ctx.topic();

            {
                let mut debates = ctx.data.debates.write().await;
                debates.clear(topic);
            }

            Ok(Response::message(
                "Debate cleared. Start a new one with `/debate <proposition>`",
            ))
        })
    }
}

/// Spween commands integrated with BotData
struct SpweenCommandWrapper;

impl Command<BotData> for SpweenCommandWrapper {
    fn definition(&self) -> CommandDef {
        CommandDef::new("spween", "Start a spween scene")
            .option(CommandOption::string("scene_id", "The scene ID to play").required())
    }

    fn execute<'a>(&'a self, ctx: CommandContext<'a, BotData>) -> BoxFuture<'a, tulip_bot::Result<Response>> {
        Box::pin(async move {
            let scene_id = ctx.args.get::<String>("scene_id")?;
            let topic = ctx.topic();
            let spween_data = &ctx.data.spweencraft;

            // Check if there's already an active session
            {
                let sessions = spween_data.sessions.read().await;
                if sessions.has_session(topic) {
                    return Ok(Response::message(
                        "There's already an active spween in this topic! End it first with `/spween_end`"
                    ));
                }
            }

            // Get the scene
            let scene = {
                let registry = spween_data.registry.read().await;
                registry.get_scene(&scene_id)
            };

            let scene = match scene {
                Some(s) => s,
                None => {
                    let available = {
                        let registry = spween_data.registry.read().await;
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
            let handler = spweencraft::BotEffectHandler::new();
            {
                let mut sessions = spween_data.sessions.write().await;
                if let Err(e) = sessions.start_session(topic, scene.clone(), handler) {
                    return Ok(Response::message(format!("Failed to start scene: {}", e)));
                }
            }

            // Get initial display
            let (text, choices) = {
                let sessions = spween_data.sessions.read().await;
                let session = sessions.get_session(topic).unwrap();
                let text = session.current_text();
                let choices = session.available_choices();
                (text, choices)
            };

            let display = spweencraft::format_spween_display(&text, &choices);

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

struct SpweenEndCommandWrapper;

impl Command<BotData> for SpweenEndCommandWrapper {
    fn definition(&self) -> CommandDef {
        CommandDef::new("spween_end", "End the current spween session")
    }

    fn execute<'a>(&'a self, ctx: CommandContext<'a, BotData>) -> BoxFuture<'a, tulip_bot::Result<Response>> {
        Box::pin(async move {
            let topic = ctx.topic();
            let spween_data = &ctx.data.spweencraft;

            let removed = {
                let mut sessions = spween_data.sessions.write().await;
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

struct SpweenListCommandWrapper;

impl Command<BotData> for SpweenListCommandWrapper {
    fn definition(&self) -> CommandDef {
        CommandDef::new("spween_list", "List all available spween scenes")
    }

    fn execute<'a>(&'a self, ctx: CommandContext<'a, BotData>) -> BoxFuture<'a, tulip_bot::Result<Response>> {
        Box::pin(async move {
            let spween_data = &ctx.data.spweencraft;
            let registry = spween_data.registry.read().await;
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

struct SpweenReloadCommandWrapper;

impl Command<BotData> for SpweenReloadCommandWrapper {
    fn definition(&self) -> CommandDef {
        CommandDef::new("spween_reload", "Reload scenes from registry channels")
    }

    fn execute<'a>(&'a self, ctx: CommandContext<'a, BotData>) -> BoxFuture<'a, tulip_bot::Result<Response>> {
        Box::pin(async move {
            let spween_data = &ctx.data.spweencraft;
            let registry = spween_data.registry.read().await;
            let count = registry.scene_count();

            Ok(Response::message(format!(
                "Currently {} scenes registered. (Full reload not yet implemented)",
                count
            )))
        })
    }
}

/// /rhai_list command - list available Rhai scripts
struct RhaiListCommand;

impl Command<BotData> for RhaiListCommand {
    fn definition(&self) -> CommandDef {
        CommandDef::new("rhai_list", "List available Rhai game scripts")
    }

    fn execute<'a>(&'a self, ctx: CommandContext<'a, BotData>) -> BoxFuture<'a, tulip_bot::Result<Response>> {
        Box::pin(async move {
            let scripts = ctx.data.rhai_games.list_scripts().await;

            if scripts.is_empty() {
                return Ok(Response::message(
                    "No Rhai scripts loaded. Add .rhai files to the scripts/ directory!",
                ));
            }

            let list = scripts
                .iter()
                .map(|(id, title, desc)| {
                    if title.is_empty() {
                        format!("- `{}`: {}", id, if desc.is_empty() { "No description" } else { desc })
                    } else {
                        format!("- `{}` ({}): {}", id, title, if desc.is_empty() { "No description" } else { desc })
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");

            Ok(Response::embed(
                RichEmbed::builder()
                    .title("Rhai Game Scripts")
                    .description(&format!("Total: {}\n\n{}", scripts.len(), list))
                    .color(0x9b59b6)
                    .build(),
            ))
        })
    }
}

/// /rhai_reload command - reload Rhai scripts
struct RhaiReloadCommand;

impl Command<BotData> for RhaiReloadCommand {
    fn definition(&self) -> CommandDef {
        CommandDef::new("rhai_reload", "Reload Rhai scripts from disk")
    }

    fn execute<'a>(&'a self, ctx: CommandContext<'a, BotData>) -> BoxFuture<'a, tulip_bot::Result<Response>> {
        Box::pin(async move {
            match ctx.data.rhai_games.reload().await {
                Ok(count) => Ok(Response::message(format!("Reloaded {} Rhai script(s)", count))),
                Err(e) => Ok(Response::message(format!("Reload failed: {}", e))),
            }
        })
    }
}

/// /rhai_end command - end current Rhai game session
struct RhaiEndCommand;

impl Command<BotData> for RhaiEndCommand {
    fn definition(&self) -> CommandDef {
        CommandDef::new("rhai_end", "End the current Rhai game session")
    }

    fn execute<'a>(&'a self, ctx: CommandContext<'a, BotData>) -> BoxFuture<'a, tulip_bot::Result<Response>> {
        Box::pin(async move {
            let topic = ctx.topic();
            if ctx.data.rhai_games.has_session(topic).await {
                ctx.data.rhai_games.end_session(topic).await;
                Ok(Response::message("Rhai game session ended."))
            } else {
                Ok(Response::message("No active Rhai game in this topic."))
            }
        })
    }
}

/// /imagine command - generate an image from a text prompt
struct ImagineCommand;

impl Command<BotData> for ImagineCommand {
    fn definition(&self) -> CommandDef {
        CommandDef::new("imagine", "Generate an image from a text prompt using FLUX")
            .option(CommandOption::string("prompt", "Description of the image to generate").required())
            .option(CommandOption::number("width", "Image width in pixels (default: 1280)"))
            .option(CommandOption::number("height", "Image height in pixels (default: 720)"))
    }

    fn execute<'a>(&'a self, ctx: CommandContext<'a, BotData>) -> BoxFuture<'a, tulip_bot::Result<Response>> {
        Box::pin(async move {
            let prompt = ctx.args.get::<String>("prompt")?;
            let width = ctx.args.get::<f64>("width").ok().map(|w| w as usize);
            let height = ctx.args.get::<f64>("height").ok().map(|h| h as usize);

            // Add working reaction
            ctx.react("art").await.ok();

            // Ensure image model is loaded
            let engine = get_or_load_image_model(
                ctx.data.image_model.clone(),
                &ctx.data.image_config,
            )
            .await
            .map_err(|e| tulip_bot::TulipError::Command(format!("Image model load failed: {}", e)))?;

            // Generate image with optional custom dimensions
            let image_path = match (width, height) {
                (Some(w), Some(h)) => engine.generate_with_size(&prompt, w, h).await,
                _ => engine.generate(&prompt).await,
            }
            .map_err(|e| tulip_bot::TulipError::Command(format!("Image generation failed: {}", e)))?;

            // Upload to Zulip
            let image_url = ctx
                .client
                .upload_file(&image_path)
                .await
                .map_err(|e| tulip_bot::TulipError::Command(format!("Failed to upload image: {}", e)))?;

            ctx.unreact("art").await.ok();
            ctx.react("frame_with_picture").await.ok();

            // Return message with embedded image
            let content = format!(
                "**Prompt:** {}\n\n![Generated image]({})",
                prompt, image_url
            );

            Ok(Response::message(content))
        })
    }
}

/// /imagine_info command - show info about the image model
struct ImagineInfoCommand;

impl Command<BotData> for ImagineInfoCommand {
    fn definition(&self) -> CommandDef {
        CommandDef::new("imagine_info", "Show information about the image generation model")
    }

    fn execute<'a>(&'a self, ctx: CommandContext<'a, BotData>) -> BoxFuture<'a, tulip_bot::Result<Response>> {
        Box::pin(async move {
            let config = &ctx.data.image_config;
            let loaded = ctx.data.image_model.read().await.is_some();

            let response = Response::embed(
                RichEmbed::builder()
                    .title("Image Model Configuration")
                    .field("Model ID", &config.model_id, false)
                    .field("Mode", if config.offloaded { "Offloaded (low VRAM)" } else { "Full (fast)" }, true)
                    .field("Default Size", &format!("{}x{}", config.default_width, config.default_height), true)
                    .field("Status", if loaded { "Loaded" } else { "Not loaded (will load on first use)" }, false)
                    .color(0x3498db)
                    .build(),
            );

            Ok(response)
        })
    }
}

/// Combined message handler for Rhai games, debates, and spweens
async fn handle_combined_message(ctx: MessageContext<'_, BotData>) -> tulip_bot::Result<Option<Response>> {
    let topic = ctx.topic();
    let content = ctx.content().trim();
    let spween_data = &ctx.data.spweencraft;
    let rhai_games = &ctx.data.rhai_games;

    // Priority 1: Check for active Rhai game session
    if rhai_games.has_session(topic).await {
        let exec_ctx = rhai_games::api::ExecutionContext {
            topic: topic.to_string(),
            channel: ctx.channel().to_string(),
            sender_name: ctx.sender_name().to_string(),
            sender_id: ctx.sender_id(),
            content: content.to_string(),
            args: HashMap::new(),
        };

        match rhai_games.handle_message(topic, &exec_ctx).await {
            Ok(Some(response)) => {
                return Ok(Some(convert_rhai_response(response)));
            }
            Ok(None) => {
                // Script didn't produce a response, continue to other handlers
            }
            Err(e) => {
                tracing::error!("Rhai script error: {}", e);
                return Ok(Some(Response::message(format!("Script error: {}", e))));
            }
        }
    }

    // Priority 2: Check if there's an active spween session
    let has_spween_session = {
        let sessions = spween_data.sessions.read().await;
        sessions.has_session(topic)
    };

    if has_spween_session {
        // Try to parse as a choice number
        if let Ok(choice_num) = content.parse::<usize>() {
            if choice_num > 0 {
                return handle_spween_choice(&ctx, choice_num - 1).await;
            }
        }
    } else {
        // Check if this is a registry stream and the message contains ```spween blocks
        if spween_data.is_registry_stream(ctx.channel()) {
            let blocks = spweencraft::extract_spween_blocks(content);
            if !blocks.is_empty() {
                return handle_spween_registration(&ctx, &blocks).await;
            }
        }
    }

    // Priority 3: Try debate handler
    handle_debate_message(ctx).await
}

/// Convert a Rhai script response to a tulip-bot Response
fn convert_rhai_response(rhai_resp: rhai_games::api::ScriptResponse) -> Response {
    if let Some(embed) = rhai_resp.embed {
        let mut builder = RichEmbed::builder();
        if let Some(title) = embed.title {
            builder = builder.title(&title);
        }
        if let Some(desc) = embed.description {
            builder = builder.description(&desc);
        }
        if let Some(color) = embed.color {
            builder = builder.color(color as u32);
        }
        if let Some(footer) = embed.footer {
            builder = builder.footer(&footer);
        }
        for field in embed.fields {
            builder = builder.field(&field.name, &field.value, field.inline);
        }
        Response::embed(builder.build())
    } else if let Some(content) = rhai_resp.content {
        Response::message(content)
    } else {
        Response::empty()
    }
}

/// Handle spween choice selection
async fn handle_spween_choice(
    ctx: &MessageContext<'_, BotData>,
    choice_num: usize,
) -> tulip_bot::Result<Option<Response>> {
    let topic = ctx.topic();
    let spween_data = &ctx.data.spweencraft;

    // Add working reaction
    ctx.react("working").await.ok();

    // Select the choice
    let result = {
        let mut sessions = spween_data.sessions.write().await;
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
                    let display = spweencraft::format_spween_display(&text, &choices);

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
        let mut sessions = spween_data.sessions.write().await;
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
async fn handle_spween_registration(
    ctx: &MessageContext<'_, BotData>,
    blocks: &[String],
) -> tulip_bot::Result<Option<Response>> {
    let stream = ctx.channel();
    let spween_data = &ctx.data.spweencraft;
    let mut registered = Vec::new();
    let mut errors = Vec::new();

    for (i, block) in blocks.iter().enumerate() {
        let filename = format!("{}/message_{}_block_{}.scene", stream, ctx.message.id, i);

        let result = {
            let mut registry = spween_data.registry.write().await;
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

/// Handle regular debate turns
async fn handle_debate_message(ctx: MessageContext<'_, BotData>) -> tulip_bot::Result<Option<Response>> {
    let topic = ctx.topic();
    let content = ctx.content();
    let sender = ctx.sender_name();

    // Check for judge shorthand
    let trimmed = content.trim().to_lowercase();
    if trimmed == "judge" || trimmed == "judge!" {
        // Let the /judge command handle it via a message
        return Ok(None);
    }

    // Get debate state
    let debate_exists = {
        let debates = ctx.data.debates.read().await;
        debates.has_debate(topic)
    };

    if !debate_exists {
        // No active debate - prompt to start one
        return Ok(Some(Response::message(
            "No active debate. Start one with `/debate <proposition>`",
        )));
    }

    // Add user's argument to history
    {
        let mut debates = ctx.data.debates.write().await;
        if let Some(debate) = debates.get_mut(topic) {
            debate.add_turn(sender, content);
        }
    }

    // Add working reaction
    ctx.react("working").await.ok();

    // Ensure opponent model is loaded
    let opponent = get_or_load_model(
        ctx.data.opponent_model.clone(),
        &ctx.data.opponent_config,
    )
    .await
    .map_err(|e| tulip_bot::TulipError::Command(format!("Model load failed: {}", e)))?;

    // Get debate state for prompt
    let debate = {
        let debates = ctx.data.debates.read().await;
        debates.get(topic).cloned()
    };

    let debate = match debate {
        Some(d) => d,
        None => return Ok(None),
    };

    // Generate opponent response
    let prompt = build_opponent_prompt(&debate, sender, content);
    let opponent_response = opponent.generate(&prompt).await.map_err(|e| {
        tulip_bot::TulipError::Command(format!("Opponent generation failed: {}", e))
    })?;

    // Add opponent's response to history
    {
        let mut debates = ctx.data.debates.write().await;
        if let Some(debate) = debates.get_mut(topic) {
            debate.add_turn("Opponent", &opponent_response);
        }
    }

    ctx.unreact("working").await.ok();
    ctx.react("white_check_mark").await.ok();

    Ok(Some(Response::message(opponent_response)))
}

/// Get or load a model
async fn get_or_load_model(
    model_slot: Arc<RwLock<Option<Arc<LlmEngine>>>>,
    config: &ModelConfig,
) -> std::result::Result<Arc<LlmEngine>, llm::LlmError> {
    // Check if already loaded
    {
        let guard = model_slot.read().await;
        if let Some(ref engine) = *guard {
            return Ok(engine.clone());
        }
    }

    // Load the model
    info!("Loading model {} ({})", config.model_id, config.quantization);
    let engine = LlmEngine::new(
        &config.model_id,
        &config.quantization,
        config.max_tokens,
        config.temperature,
    )
    .await?;

    let engine = Arc::new(engine);

    // Store it
    {
        let mut guard = model_slot.write().await;
        *guard = Some(engine.clone());
    }

    Ok(engine)
}

/// Get or load an image model
async fn get_or_load_image_model(
    model_slot: Arc<RwLock<Option<Arc<ImageEngine>>>>,
    config: &ImageModelConfig,
) -> std::result::Result<Arc<ImageEngine>, llm::LlmError> {
    // Check if already loaded
    {
        let guard = model_slot.read().await;
        if let Some(ref engine) = *guard {
            return Ok(engine.clone());
        }
    }

    // Load the model
    info!(
        "Loading image model {} (offloaded: {})",
        config.model_id, config.offloaded
    );
    let engine = ImageEngine::new(
        &config.model_id,
        config.offloaded,
        config.default_width,
        config.default_height,
    )
    .await?;

    let engine = Arc::new(engine);

    // Store it
    {
        let mut guard = model_slot.write().await;
        *guard = Some(engine.clone());
    }

    Ok(engine)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    let env_filter = tracing_subscriber::EnvFilter::from_default_env()
        .add_directive("zwobot=info".parse()?);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting zwobot debate club...");

    // Load configs
    let app_config = AppConfig::load()?;
    let tulip_config = TulipConfig::from_zuliprc("zuliprc")?;

    info!("Monitoring channel: {}", app_config.channel);

    // Create spweencraft data
    let registry_streams = vec!["spween-library".to_string()];
    let spween_data = Arc::new(SpweencraftData::new(registry_streams));

    // Load bundled spween
    if let Err(e) = spween_data.load_bundled_scene(LONG_SPWEEN, "long.spween").await {
        tracing::error!("Failed to load bundled scene: {}", e);
    } else {
        info!("Loaded bundled scene: long.spween");
    }

    // Initialize Rhai games engine
    let rhai_games = match RhaiGamesData::new("scripts/").await {
        Ok(data) => Arc::new(data),
        Err(e) => {
            tracing::error!("Failed to initialize Rhai games: {}", e);
            return Err(anyhow::anyhow!("Rhai games init failed: {}", e));
        }
    };
    info!("Rhai games engine initialized");

    // Create shared data
    let data = BotData {
        debates: Arc::new(RwLock::new(DebateManager::new())),
        opponent_model: Arc::new(RwLock::new(None)),
        judge_model: Arc::new(RwLock::new(None)),
        image_model: Arc::new(RwLock::new(None)),
        opponent_config: app_config.debate_opponent,
        judge_config: app_config.debate_judge,
        image_config: app_config.image_model,
        spweencraft: spween_data,
        rhai_games,
    };

    // Build framework
    let framework = Framework::builder()
        .data(data)
        .channel(&app_config.channel)
        .command(DebateCommand)
        .command(JudgeCommand)
        .command(ClearCommand)
        .command(SpweenCommandWrapper)
        .command(SpweenEndCommandWrapper)
        .command(SpweenListCommandWrapper)
        .command(SpweenReloadCommandWrapper)
        .command(RhaiListCommand)
        .command(RhaiReloadCommand)
        .command(RhaiEndCommand)
        .command(ImagineCommand)
        .command(ImagineInfoCommand)
        .on_message(|ctx| Box::pin(handle_combined_message(ctx)))
        .build(tulip_config)
        .await?;

    // Register commands with Tulip
    framework.register_commands().await?;

    // Run the bot
    framework.run().await?;

    Ok(())
}
