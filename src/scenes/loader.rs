//! Scene loading and management.

use crate::scenes::handler::SceneContext;
use crate::world::{ActiveScene, WorldState};
use smol_str::SmolStr;
use spween::{parse, Runtime, Scene};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tracing::{debug, error, info, trace};

/// Manages loading and caching of scene files.
#[derive(Default)]
pub struct SceneManager {
    /// Cached parsed scenes: scene_id -> Scene
    cache: HashMap<SmolStr, Scene>,
    /// Directory containing scene files
    scenes_dir: String,
}

impl SceneManager {
    pub fn new(scenes_dir: &str) -> Self {
        Self {
            cache: HashMap::new(),
            scenes_dir: scenes_dir.to_string(),
        }
    }

    /// Load a scene by ID, using cache if available.
    pub fn load_scene(&mut self, scene_id: &str) -> anyhow::Result<&Scene> {
        let scene_id = SmolStr::new(scene_id);

        // Return cached if available
        if self.cache.contains_key(&scene_id) {
            trace!("Using cached scene: {}", scene_id);
            return Ok(self.cache.get(&scene_id).unwrap());
        }

        // Load from file
        let path = format!("{}/{}.scene", self.scenes_dir, scene_id);
        debug!("Loading scene from file: {}", path);
        let source = fs::read_to_string(&path)?;
        let scene = parse(&source, &path)?;

        info!("Loaded scene: {} ({})", scene.meta.title, scene_id);
        debug!("Scene has {} passages", scene.passages.len());

        self.cache.insert(scene_id.clone(), scene);
        Ok(self.cache.get(&scene_id).unwrap())
    }

    /// Check if a scene file exists.
    pub fn scene_exists(&self, scene_id: &str) -> bool {
        let path = format!("{}/{}.scene", self.scenes_dir, scene_id);
        Path::new(&path).exists()
    }

    /// Load all scenes from the scenes directory.
    pub fn load_all(&mut self) -> anyhow::Result<()> {
        let path = Path::new(&self.scenes_dir);
        if !path.exists() {
            return Ok(());
        }

        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let file_path = entry.path();

            if file_path.extension().map_or(false, |ext| ext == "scene") {
                if let Some(stem) = file_path.file_stem().and_then(|s| s.to_str()) {
                    match self.load_scene(stem) {
                        Ok(_) => {}
                        Err(e) => error!("Failed to load scene {}: {}", stem, e),
                    }
                }
            }
        }

        Ok(())
    }
}

/// Result of entering or continuing a scene.
pub struct SceneOutput {
    /// The prose text to display
    pub prose: String,
    /// Available choices (index, text, available)
    pub choices: Vec<(usize, String, bool)>,
    /// Whether the scene has ended
    pub ended: bool,
    /// Summary of what happened (for turn coordinator)
    pub summary: String,
}

/// Enter a new scene for a character.
pub fn enter_scene(
    world: &mut WorldState,
    character: &SmolStr,
    scene: &Scene,
) -> anyhow::Result<SceneOutput> {
    debug!("Entering scene '{}' for character {}", scene.meta.id, character);

    let context = SceneContext::new(world, character.clone());
    let runtime = Runtime::new(scene, context)?;

    let prose = runtime.current_prose().unwrap_or_default();
    let choices: Vec<(usize, String, bool)> = runtime
        .current_choices()
        .into_iter()
        .map(|c| (c.index, c.text.to_string(), c.available))
        .collect();
    let ended = runtime.is_ended();
    let current_passage_name = runtime
        .current_passage()
        .map(|p| p.name.clone())
        .unwrap_or_else(|| SmolStr::new("intro"));
    let scene_id = scene.meta.id.clone();
    let scene_title = scene.meta.title.clone();

    debug!("Scene entered at passage: {} ({} choices)", current_passage_name, choices.len());
    trace!("Prose length: {} chars", prose.len());

    // Extract context (releasing the borrow) before updating world
    let _context = runtime.into_handler();

    // Store active scene state (now safe since runtime is consumed)
    if !ended {
        if let Some(char_state) = world.get_character_mut(character) {
            char_state.active_scene = Some(ActiveScene {
                scene_id,
                current_passage: current_passage_name,
            });
            trace!("Stored active scene in character state");
        }
    } else {
        debug!("Scene ended immediately (no active scene stored)");
    }

    Ok(SceneOutput {
        prose,
        choices,
        ended,
        summary: format!("Began conversation ({})", scene_title),
    })
}

/// Continue a scene by selecting a choice.
pub fn continue_scene(
    world: &mut WorldState,
    character: &SmolStr,
    scene: &Scene,
    choice_index: usize,
) -> anyhow::Result<SceneOutput> {
    debug!("Continuing scene '{}' for character {} (choice {})", scene.meta.id, character, choice_index);

    // Get current passage to restore position
    let current_passage = world
        .get_character(character)
        .and_then(|cs| cs.active_scene.as_ref())
        .map(|as_| as_.current_passage.clone());

    if let Some(ref passage) = current_passage {
        debug!("Restoring scene to passage: {}", passage);
    }

    let context = SceneContext::new(world, character.clone());
    let mut runtime = Runtime::new(scene, context)?;

    // Jump to current passage if we're not at the start
    if let Some(passage) = &current_passage {
        if passage.as_str() != "intro" {
            runtime.jump_to(passage)?;
        }
    }

    // Get choice text before selecting (for summary)
    let choice_text = runtime
        .current_choices()
        .get(choice_index)
        .map(|c| c.text.to_string())
        .unwrap_or_else(|| format!("choice {}", choice_index));

    debug!("Selected choice: '{}'", choice_text);

    // Select the choice
    runtime.select_choice(choice_index)?;

    let prose = runtime.current_prose().unwrap_or_default();
    let choices: Vec<(usize, String, bool)> = runtime
        .current_choices()
        .into_iter()
        .map(|c| (c.index, c.text.to_string(), c.available))
        .collect();
    let ended = runtime.is_ended();
    let new_passage_name = runtime.current_passage().map(|p| p.name.clone());

    if let Some(ref passage) = new_passage_name {
        debug!("Moved to passage: {} ({} choices)", passage, choices.len());
    }

    // Extract context (releasing the borrow) before updating world
    let _context = runtime.into_handler();

    // Update or clear active scene (now safe since runtime is consumed)
    if ended {
        debug!("Scene ended, clearing active scene");
        if let Some(char_state) = world.get_character_mut(character) {
            char_state.active_scene = None;
        }
    } else if let Some(passage_name) = new_passage_name {
        if let Some(char_state) = world.get_character_mut(character) {
            if let Some(active) = &mut char_state.active_scene {
                active.current_passage = passage_name;
            }
        }
    }

    Ok(SceneOutput {
        prose,
        choices,
        ended,
        summary: format!("Chose: {}", choice_text),
    })
}

/// Format scene output for display in Zulip.
pub fn format_scene_for_zulip(output: &SceneOutput) -> String {
    let mut result = String::new();

    // Prose
    result.push_str(&output.prose);

    // Choices (if any and not ended)
    if !output.ended && !output.choices.is_empty() {
        result.push_str("\n\n---\n");
        for (idx, text, available) in &output.choices {
            if *available {
                result.push_str(&format!("**{}**: {}\n", idx, text));
            } else {
                result.push_str(&format!("~~{}~~: {} *(unavailable)*\n", idx, text));
            }
        }
        result.push_str("\n*Use `<tool>choose N</tool>` to select a choice.*");
    }

    result
}

/// Format scene context for inclusion in prompts.
pub fn format_scene_for_prompt(output: &SceneOutput) -> String {
    let mut result = String::new();

    result.push_str("**Active Dialogue:**\n\n");
    result.push_str(&output.prose);
    result.push('\n');

    if !output.ended && !output.choices.is_empty() {
        result.push_str("\n**Your choices:**\n");
        for (idx, text, available) in &output.choices {
            if *available {
                result.push_str(&format!("- `choose {}` - {}\n", idx, text));
            }
        }
    }

    result
}
