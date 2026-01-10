//! Zwobot - Debate club bot using Tulip framework
//!
//! A bot that engages users in structured debates using local LLMs.

use debate::{build_judge_prompt, build_opponent_prompt, DebateManager};
use llm::{ImageEngine, LlmEngine};
use rhai_games::RhaiGamesData;
use rpg::{combat_key, CharacterSheet, DiceRoll, RpgData};
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
    #[serde(default)]
    ctc_detector: Option<CtcDetectorConfig>,
}

/// Cinnamon Toast Crunch Detector configuration
#[derive(Debug, Deserialize, Clone)]
struct CtcDetectorConfig {
    /// Model for asking "is this the reason why kids love cinnamon toast crunch?"
    question_model: ModelConfig,
    /// Model for evaluating if the response was affirmative
    evaluator_model: ModelConfig,
    /// Puppet name to use when responding
    puppet_name: String,
    /// Optional puppet avatar URL
    #[serde(default)]
    puppet_avatar_url: Option<String>,
    /// Optional puppet color (hex format: #RGB or #RRGGBB)
    #[serde(default)]
    puppet_color: Option<String>,
    /// Optional channel to restrict detection to (if not set, runs on all channels)
    #[serde(default)]
    channel: Option<String>,
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
    rpg: Arc<RpgData>,
    // Cinnamon Toast Crunch detector
    ctc_detector: Option<CtcDetectorData>,
}

/// Data for the Cinnamon Toast Crunch detector
#[derive(Clone)]
struct CtcDetectorData {
    question_model: Arc<RwLock<Option<Arc<LlmEngine>>>>,
    evaluator_model: Arc<RwLock<Option<Arc<LlmEngine>>>>,
    question_config: ModelConfig,
    evaluator_config: ModelConfig,
    puppet_name: String,
    puppet_avatar_url: Option<String>,
    puppet_color: Option<String>,
    channel: Option<String>,
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

            // Get the scene first (before acquiring session lock)
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

            // Check and create session atomically under single write lock
            let handler = spweencraft::BotEffectHandler::new();
            {
                let mut sessions = spween_data.sessions.write().await;

                // Check if there's already an active session
                if sessions.has_session(topic) {
                    return Ok(Response::message(
                        "There's already an active spween in this topic! End it first with `/spween_end`"
                    ));
                }

                // Create session (still under write lock)
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

// =============================================================================
// RPG Commands
// =============================================================================

/// Colors for roll embeds
const COLOR_CRIT: u32 = 0x2ecc71;     // Green for nat 20
const COLOR_FAIL: u32 = 0xe74c3c;     // Red for nat 1
const COLOR_NORMAL: u32 = 0x3498db;   // Blue for normal
const COLOR_STATS: u32 = 0x9b59b6;    // Purple for character stats
const COLOR_COMBAT: u32 = 0xe67e22;   // Orange for combat

/// /roll command - roll dice
struct RollCommand;

impl Command<BotData> for RollCommand {
    fn definition(&self) -> CommandDef {
        CommandDef::new("roll", "Roll dice (e.g., 2d6+3, 4d6kh3, adv)")
            .option(CommandOption::string("dice", "Dice notation").required())
            .option(CommandOption::string("label", "What you're rolling for"))
    }

    fn execute<'a>(&'a self, ctx: CommandContext<'a, BotData>) -> BoxFuture<'a, tulip_bot::Result<Response>> {
        Box::pin(async move {
            let dice_str: String = ctx.args.get("dice")?;
            let label: Option<String> = ctx.args.get_optional("label")?;

            match DiceRoll::parse(&dice_str) {
                Ok(dice) => {
                    let result = dice.roll();
                    let embed = build_roll_embed(&result, label.as_deref(), ctx.sender_name(), false);
                    Ok(Response::embed(embed))
                }
                Err(e) => Ok(Response::message(format!("Invalid dice notation: {}", e))),
            }
        })
    }
}

/// /gmroll command - secret roll (whispered)
struct GmRollCommand;

impl Command<BotData> for GmRollCommand {
    fn definition(&self) -> CommandDef {
        CommandDef::new("gmroll", "Roll dice secretly (only you see the result)")
            .option(CommandOption::string("dice", "Dice notation").required())
            .option(CommandOption::string("label", "What you're rolling for"))
    }

    fn execute<'a>(&'a self, ctx: CommandContext<'a, BotData>) -> BoxFuture<'a, tulip_bot::Result<Response>> {
        Box::pin(async move {
            let dice_str: String = ctx.args.get("dice")?;
            let label: Option<String> = ctx.args.get_optional("label")?;

            match DiceRoll::parse(&dice_str) {
                Ok(dice) => {
                    let result = dice.roll();
                    let embed = build_roll_embed(&result, label.as_deref(), ctx.sender_name(), true);
                    Ok(Response::embed(embed).make_private(&[ctx.sender_id()]))
                }
                Err(e) => Ok(Response::ephemeral(format!("Invalid dice notation: {}", e))),
            }
        })
    }
}

/// Build a rich embed for dice roll results
fn build_roll_embed(result: &rpg::RollResult, label: Option<&str>, roller: &str, secret: bool) -> RichEmbed {
    let mut builder = RichEmbed::builder();

    // Title with optional label
    let title = if let Some(label) = label {
        format!("{}Roll: {} ({})", if secret { "Secret " } else { "" }, result.notation, label)
    } else {
        format!("{}Roll: {}", if secret { "Secret " } else { "" }, result.notation)
    };
    builder = builder.title(title);

    // Check for crits on d20
    let is_d20 = result.dice.len() == 1 && result.notation.contains("d20");
    let first_die = result.dice.first().map(|d| d.value);

    let color = if is_d20 {
        match first_die {
            Some(20) => COLOR_CRIT,
            Some(1) => COLOR_FAIL,
            _ => COLOR_NORMAL,
        }
    } else {
        COLOR_NORMAL
    };
    builder = builder.color(color);

    // Dice results
    builder = builder.field("Dice", result.format_dice(), false);

    // Modifier if any
    if result.modifier != 0 {
        let mod_str = if result.modifier > 0 {
            format!("+{}", result.modifier)
        } else {
            format!("{}", result.modifier)
        };
        builder = builder.field("Modifier", mod_str, true);
    }

    // Total
    let total_str = if is_d20 && first_die == Some(20) {
        format!("**{}** (Critical!)", result.total)
    } else if is_d20 && first_die == Some(1) {
        format!("**{}** (Critical Fail!)", result.total)
    } else {
        format!("**{}**", result.total)
    };
    builder = builder.field("Total", total_str, true);

    builder = builder.footer(format!("Rolled by {}", roller));
    builder.build()
}

/// /stats command - character sheet management
struct StatsCommand;

impl Command<BotData> for StatsCommand {
    fn definition(&self) -> CommandDef {
        CommandDef::new("stats", "View or modify your character stats")
            .option(
                CommandOption::string("action", "Action to take")
                    .choice("view", "view")
                    .choice("set", "set")
                    .choice("hurt", "hurt")
                    .choice("heal", "heal"),
            )
            .option(CommandOption::string("stat", "Stat name (hp, str, dex, con, int, wis, cha)"))
            .option(CommandOption::number("value", "Value to set or amount to change"))
    }

    fn execute<'a>(&'a self, ctx: CommandContext<'a, BotData>) -> BoxFuture<'a, tulip_bot::Result<Response>> {
        Box::pin(async move {
            let action: String = ctx.args.get_optional("action")?.unwrap_or_else(|| "view".to_string());
            let stat: Option<String> = ctx.args.get_optional("stat")?;
            let value: Option<i32> = ctx.args.get_optional("value")?;

            let user_id = ctx.sender_id();
            let user_name = ctx.sender_name().to_string();
            let rpg = &ctx.data.rpg;

            match action.as_str() {
                "view" => {
                    if let Some(sheet) = rpg.get_character(user_id).await {
                        Ok(Response::embed(build_stats_embed(&sheet)))
                    } else {
                        Ok(Response::message("No character sheet found. Use `/stats set hp 20` to start."))
                    }
                }
                "set" => {
                    let stat = stat.ok_or_else(|| tulip_bot::TulipError::MissingArgument("stat".to_string()))?;
                    let value = value.ok_or_else(|| tulip_bot::TulipError::MissingArgument("value".to_string()))?;

                    let mut sheet = rpg.get_or_create_character(user_id, user_name).await;
                    sheet.set_stat(&stat, value);
                    rpg.update_character(sheet.clone()).await;

                    if let Err(e) = rpg.save().await {
                        tracing::error!("Failed to save RPG data: {}", e);
                    }

                    Ok(Response::embed(
                        RichEmbed::builder()
                            .title(format!("Set {} = {}", stat.to_uppercase(), value))
                            .description(sheet.hp_bar())
                            .color(COLOR_STATS)
                            .build()
                    ))
                }
                "hurt" => {
                    let amount = value.ok_or_else(|| tulip_bot::TulipError::MissingArgument("value".to_string()))?;

                    if let Some(mut sheet) = rpg.get_character(user_id).await {
                        let new_hp = sheet.hurt(amount);
                        let hp_bar = sheet.hp_bar();
                        rpg.update_character(sheet).await;

                        if let Err(e) = rpg.save().await {
                            tracing::error!("Failed to save RPG data: {}", e);
                        }

                        let msg = if new_hp <= 0 {
                            format!("{} takes {} damage and falls unconscious!", user_name, amount)
                        } else {
                            format!("{} takes {} damage!", user_name, amount)
                        };

                        Ok(Response::embed(
                            RichEmbed::builder()
                                .title(msg)
                                .description(hp_bar)
                                .color(COLOR_FAIL)
                                .build()
                        ))
                    } else {
                        Ok(Response::message("No character sheet. Use `/stats set hp 20` first."))
                    }
                }
                "heal" => {
                    let amount = value.ok_or_else(|| tulip_bot::TulipError::MissingArgument("value".to_string()))?;

                    if let Some(mut sheet) = rpg.get_character(user_id).await {
                        sheet.heal(amount);
                        let hp_bar = sheet.hp_bar();
                        rpg.update_character(sheet).await;

                        if let Err(e) = rpg.save().await {
                            tracing::error!("Failed to save RPG data: {}", e);
                        }

                        Ok(Response::embed(
                            RichEmbed::builder()
                                .title(format!("{} heals {} HP!", user_name, amount))
                                .description(hp_bar)
                                .color(COLOR_CRIT)
                                .build()
                        ))
                    } else {
                        Ok(Response::message("No character sheet. Use `/stats set hp 20` first."))
                    }
                }
                _ => Ok(Response::message(format!("Unknown action: {}", action))),
            }
        })
    }
}

/// Build a character sheet embed
fn build_stats_embed(sheet: &CharacterSheet) -> RichEmbed {
    let mut builder = RichEmbed::builder()
        .title(format!("Character: {}", sheet.name))
        .color(COLOR_STATS)
        .description(sheet.hp_bar());

    // Add core stats if they exist
    for stat in ["str", "dex", "con", "int", "wis", "cha"] {
        if let Some(formatted) = sheet.format_stat(stat) {
            if let Some(value) = formatted.split(": ").nth(1) {
                builder = builder.field(stat.to_uppercase(), value, true);
            }
        }
    }

    builder.build()
}

/// /initiative command - roll initiative
struct InitiativeCommand;

impl Command<BotData> for InitiativeCommand {
    fn definition(&self) -> CommandDef {
        CommandDef::new("initiative", "Roll initiative to join combat")
            .option(CommandOption::number("bonus", "Initiative modifier (default 0)"))
    }

    fn execute<'a>(&'a self, ctx: CommandContext<'a, BotData>) -> BoxFuture<'a, tulip_bot::Result<Response>> {
        Box::pin(async move {
            let bonus: i32 = ctx.args.get_optional("bonus")?.unwrap_or(0);
            let user_id = ctx.sender_id();
            let user_name = ctx.sender_name().to_string();
            let key = combat_key(ctx.channel(), ctx.topic());
            let rpg = &ctx.data.rpg;

            // Roll 1d20 + bonus
            let dice = DiceRoll::parse(&format!("1d20{:+}", bonus)).unwrap();
            let result = dice.roll();
            let initiative = result.total;

            // Add to combat state
            let mut state = rpg.get_or_create_combat(&key).await;
            state.add_combatant(user_id, user_name.clone(), initiative, false);
            let turn_order = state.format_turn_order();
            let combatant_count = state.combatants.len();
            rpg.update_combat(&key, state).await;

            if let Err(e) = rpg.save().await {
                tracing::error!("Failed to save RPG data: {}", e);
            }

            Ok(Response::embed(
                RichEmbed::builder()
                    .title(format!("{} rolls initiative!", user_name))
                    .color(COLOR_COMBAT)
                    .field("Roll", format!("1d20{:+} = **{}**", bonus, initiative), false)
                    .field(format!("Turn Order ({} combatants)", combatant_count), turn_order, false)
                    .build()
            ))
        })
    }
}

/// /turn command - turn management
struct TurnCommand;

impl Command<BotData> for TurnCommand {
    fn definition(&self) -> CommandDef {
        CommandDef::new("turn", "Manage combat turns")
            .option(
                CommandOption::string("action", "Action to take")
                    .choice("start", "start")
                    .choice("next", "next")
                    .choice("list", "list")
                    .choice("end", "end")
                    .required(),
            )
    }

    fn execute<'a>(&'a self, ctx: CommandContext<'a, BotData>) -> BoxFuture<'a, tulip_bot::Result<Response>> {
        Box::pin(async move {
            let action: String = ctx.args.get("action")?;
            let key = combat_key(ctx.channel(), ctx.topic());
            let rpg = &ctx.data.rpg;

            match action.as_str() {
                "start" => {
                    if let Some(mut state) = rpg.get_combat(&key).await {
                        if state.combatants.is_empty() {
                            return Ok(Response::message("No combatants! Use `/initiative` to join first."));
                        }
                        state.start();
                        let current = state.current().map(|c| c.name.clone()).unwrap_or_default();
                        let turn_order = state.format_turn_order();
                        let round = state.round;
                        rpg.update_combat(&key, state).await;

                        if let Err(e) = rpg.save().await {
                            tracing::error!("Failed to save RPG data: {}", e);
                        }

                        Ok(Response::embed(
                            RichEmbed::builder()
                                .title("Combat Begins!")
                                .color(COLOR_COMBAT)
                                .field(format!("Round {}", round), turn_order, false)
                                .footer(format!("{}'s turn", current))
                                .build()
                        ))
                    } else {
                        Ok(Response::message("No combatants! Use `/initiative` to join first."))
                    }
                }
                "next" => {
                    if let Some(mut state) = rpg.get_combat(&key).await {
                        if !state.active {
                            return Ok(Response::message("Combat hasn't started. Use `/turn start`."));
                        }
                        let prev = state.current().map(|c| c.name.clone()).unwrap_or_default();
                        state.next_turn();
                        let next = state.current().map(|c| c.name.clone()).unwrap_or_default();
                        let turn_order = state.format_turn_order();
                        let round = state.round;
                        rpg.update_combat(&key, state).await;

                        if let Err(e) = rpg.save().await {
                            tracing::error!("Failed to save RPG data: {}", e);
                        }

                        Ok(Response::embed(
                            RichEmbed::builder()
                                .title(format!("{} ends their turn", prev))
                                .color(COLOR_COMBAT)
                                .field(format!("Round {}", round), turn_order, false)
                                .footer(format!("{}'s turn", next))
                                .build()
                        ))
                    } else {
                        Ok(Response::message("No combat in this topic. Use `/initiative` first."))
                    }
                }
                "list" => {
                    if let Some(state) = rpg.get_combat(&key).await {
                        let status = if state.active {
                            format!("Round {}", state.round)
                        } else {
                            "Not started".to_string()
                        };
                        let turn_order = state.format_turn_order();
                        let current = state.current().map(|c| format!("{}'s turn", c.name));

                        let mut builder = RichEmbed::builder()
                            .title("Turn Order")
                            .color(COLOR_COMBAT)
                            .field("Status", status, true)
                            .field("Combatants", turn_order, false);

                        if let Some(current) = current {
                            builder = builder.footer(current);
                        }

                        Ok(Response::embed(builder.build()))
                    } else {
                        Ok(Response::message("No combat in this topic. Use `/initiative` to start."))
                    }
                }
                "end" => {
                    rpg.remove_combat(&key).await;

                    if let Err(e) = rpg.save().await {
                        tracing::error!("Failed to save RPG data: {}", e);
                    }

                    Ok(Response::embed(
                        RichEmbed::builder()
                            .title("Combat Ended")
                            .color(COLOR_COMBAT)
                            .description("Turn order cleared.")
                            .build()
                    ))
                }
                _ => Ok(Response::message(format!("Unknown action: {}", action))),
            }
        })
    }
}

/// /loot command - random tables
struct LootCommand;

impl Command<BotData> for LootCommand {
    fn definition(&self) -> CommandDef {
        CommandDef::new("loot", "Roll on a random table")
            .option(
                CommandOption::string("table", "Which table to roll on")
                    .choice("treasure", "treasure")
                    .choice("encounter", "encounter")
                    .choice("trinket", "trinket")
                    .choice("npc", "npc")
                    .required(),
            )
    }

    fn execute<'a>(&'a self, ctx: CommandContext<'a, BotData>) -> BoxFuture<'a, tulip_bot::Result<Response>> {
        Box::pin(async move {
            let table: String = ctx.args.get("table")?;
            let result = rpg::roll_by_name(&table);

            let (title, color, emoji) = match table.as_str() {
                "treasure" => ("Treasure Found!", 0xf1c40f, "💰"),
                "encounter" => ("Encounter!", 0xe74c3c, "⚔️"),
                "trinket" => ("Strange Trinket", 0x9b59b6, "🔮"),
                "npc" => ("You Meet...", 0x3498db, "👤"),
                _ => ("Result", 0x95a5a6, "🎲"),
            };

            Ok(Response::embed(
                RichEmbed::builder()
                    .title(format!("{} {}", emoji, title))
                    .color(color)
                    .description(result)
                    .footer(format!("Rolled by {} on {} table", ctx.sender_name(), table))
                    .build()
            ))
        })
    }
}

// =============================================================================
// Message Handlers
// =============================================================================

/// Combined message handler for Rhai games, debates, spweens, and CTC detector
async fn handle_combined_message(ctx: MessageContext<'_, BotData>) -> tulip_bot::Result<Option<Response>> {
    let topic = ctx.topic();
    let content = ctx.content().trim();
    let spween_data = &ctx.data.spweencraft;
    let rhai_games = &ctx.data.rhai_games;

    // Run CTC detector in background (doesn't block other handlers)
    // Only runs if configured and channel matches (or no channel restriction)
    if let Some(ctc_data) = &ctx.data.ctc_detector {
        let should_run = ctc_data
            .channel
            .as_ref()
            .map_or(true, |ch| ch == ctx.channel());

        if should_run {
            let ctc_ctx_channel = ctx.channel().to_string();
            let ctc_ctx_topic = topic.to_string();
            let ctc_ctx_content = content.to_string();
            let ctc_ctx_message_id = ctx.message.id;
            let ctc_ctx_client = ctx.client.clone();
            let ctc_data = ctc_data.clone();

            tokio::spawn(async move {
                if let Err(e) = run_ctc_detector_background(
                    &ctc_ctx_channel,
                    &ctc_ctx_topic,
                    &ctc_ctx_content,
                    ctc_ctx_message_id,
                    &ctc_ctx_client,
                    &ctc_data,
                )
                .await
                {
                    tracing::error!("CTC detector error: {}", e);
                }
            });
        }
    }

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
                return Ok(Some(convert_rhai_response_with_upload(response, ctx.client).await));
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
/// If the response contains a local image path, uploads it first
async fn convert_rhai_response_with_upload(
    rhai_resp: rhai_games::api::ScriptResponse,
    client: &TulipClient,
) -> Response {
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

        // Handle image - upload if it's a local file path
        if let Some(ref image_path) = embed.image {
            if image_path.starts_with("http://") || image_path.starts_with("https://") {
                // Already a URL
                builder = builder.image(image_path);
            } else if std::path::Path::new(image_path).exists() {
                // Local file - upload it
                match client.upload_file(image_path).await {
                    Ok(url) => {
                        builder = builder.image(&url);
                    }
                    Err(e) => {
                        tracing::error!("Failed to upload image {}: {}", image_path, e);
                        // Just log the error - can't modify description without accessing private fields
                    }
                }
            }
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
        // No active debate - silently ignore regular messages
        // Users can start a debate with /debate command
        return Ok(None);
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

/// Run CTC detector in background
///
/// Watches every message and asks an LLM "is this the reason why kids love
/// cinnamon toast crunch?" then asks another model to evaluate if the response
/// was affirmative. If so, responds as a puppet.
async fn run_ctc_detector_background(
    channel: &str,
    topic: &str,
    content: &str,
    message_id: i64,
    client: &TulipClient,
    ctc_data: &CtcDetectorData,
) -> anyhow::Result<()> {
    if content.is_empty() {
        return Ok(());
    }

    // Load the question model
    let question_model = get_or_load_model(
        ctc_data.question_model.clone(),
        &ctc_data.question_config,
    )
    .await?;

    // Ask the question model
    let question_prompt = format!(
        r#"You are analyzing whether something could be "the reason why kids love Cinnamon Toast Crunch."

The user said: "{}"

Is this the reason why kids love Cinnamon Toast Crunch? Explain your reasoning briefly."#,
        content
    );

    let question_response = question_model.generate(&question_prompt).await?;

    tracing::debug!("CTC question response: {}", question_response);

    // Load the evaluator model
    let evaluator_model = get_or_load_model(
        ctc_data.evaluator_model.clone(),
        &ctc_data.evaluator_config,
    )
    .await?;

    // Ask the evaluator if the response was affirmative (using constrained decoding)
    let evaluator_prompt = format!(
        r#"Read this response and determine if it answers YES or AFFIRMATIVELY to the question "Is this the reason why kids love Cinnamon Toast Crunch?"

Response to evaluate:
"{}"

Reply with ONLY "YES" if the response is affirmative/positive, or "NO" if it is negative/dismissive."#,
        question_response
    );

    // Use constrained decoding to force YES or NO output
    let evaluator_response = evaluator_model
        .generate_constrained(&evaluator_prompt, r"^\s*(YES|NO)\.?\s*$")
        .await?;

    let is_affirmative = evaluator_response.trim().trim_end_matches('.').to_uppercase() == "YES";
    tracing::debug!("CTC evaluator response: {} (affirmative: {})", evaluator_response, is_affirmative);

    if is_affirmative {
        // Respond as the puppet!
        client.add_reaction(message_id, "bowl_with_spoon").await.ok();

        let puppet_message = format!(
            "**THIS** is the reason why kids love Cinnamon Toast Crunch!\n\n> {}\n\n{}",
            content,
            question_response
        );

        client
            .send_message_as_puppet(
                channel,
                topic,
                &puppet_message,
                &ctc_data.puppet_name,
                ctc_data.puppet_avatar_url.as_deref(),
                ctc_data.puppet_color.as_deref(),
            )
            .await?;
    }

    Ok(())
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
    let (rhai_games, mut timer_response_rx) = match RhaiGamesData::new("scripts/").await {
        Ok(data) => data,
        Err(e) => {
            tracing::error!("Failed to initialize Rhai games: {}", e);
            return Err(anyhow::anyhow!("Rhai games init failed: {}", e));
        }
    };
    let rhai_games = Arc::new(rhai_games);

    // Start Rhai timer loop
    rhai_games.start_timer_loop();
    info!("Rhai games engine initialized");

    // Create CTC detector data if configured
    let ctc_detector = app_config.ctc_detector.map(|config| {
        info!(
            "CTC Detector enabled with puppet '{}' (channel: {:?})",
            config.puppet_name,
            config.channel
        );
        CtcDetectorData {
            question_model: Arc::new(RwLock::new(None)),
            evaluator_model: Arc::new(RwLock::new(None)),
            question_config: config.question_model,
            evaluator_config: config.evaluator_model,
            puppet_name: config.puppet_name,
            puppet_avatar_url: config.puppet_avatar_url,
            puppet_color: config.puppet_color,
            channel: config.channel,
        }
    });

    // Initialize RPG data
    let rpg_data = RpgData::new("data/rpg");
    if let Err(e) = rpg_data.load().await {
        tracing::warn!("Failed to load RPG data (may not exist yet): {}", e);
    } else {
        info!("Loaded RPG data from data/rpg");
    }

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
        rpg: Arc::new(rpg_data),
        ctc_detector,
    };

    // Build framework
    let framework = Framework::builder()
        .data(data)
        .channel(&app_config.channel)
        // Debate commands
        .command(DebateCommand)
        .command(JudgeCommand)
        .command(ClearCommand)
        // Spweencraft commands
        .command(SpweenCommandWrapper)
        .command(SpweenEndCommandWrapper)
        .command(SpweenListCommandWrapper)
        .command(SpweenReloadCommandWrapper)
        // Rhai games commands
        .command(RhaiListCommand)
        .command(RhaiReloadCommand)
        .command(RhaiEndCommand)
        // Image generation
        .command(ImagineCommand)
        .command(ImagineInfoCommand)
        // RPG commands
        .command(RollCommand)
        .command(GmRollCommand)
        .command(StatsCommand)
        .command(InitiativeCommand)
        .command(TurnCommand)
        .command(LootCommand)
        .on_message(|ctx| Box::pin(handle_combined_message(ctx)))
        .build(tulip_config)
        .await?;

    // Register commands with Tulip
    framework.register_commands().await?;

    // Spawn timer response handler
    let client_for_timers = framework.client().clone();
    tokio::spawn(async move {
        while let Some(timer_resp) = timer_response_rx.recv().await {
            // Convert script response to tulip response (with image upload support)
            let response = convert_rhai_response_with_upload(timer_resp.response, &client_for_timers).await;

            // Get channel - use the stored channel or fall back to a default
            let channel = timer_resp.channel.as_deref().unwrap_or("general");
            let topic = &timer_resp.topic;

            // Send the response
            if let Err(e) = client_for_timers.send_response(channel, topic, &response).await {
                tracing::error!("Failed to send timer response to {}/{}: {}", channel, topic, e);
            } else {
                tracing::debug!("Sent timer response to {}/{}", channel, topic);
            }
        }
    });

    // Run the bot
    framework.run().await?;

    Ok(())
}
