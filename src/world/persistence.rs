use super::state::WorldState;
use std::fs;
use std::path::Path;
use tracing::info;

impl WorldState {
    /// Load world state from a RON file
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let contents = fs::read_to_string(path)?;
        let world: WorldState = ron::from_str(&contents)?;
        Ok(world)
    }

    /// Load world state from save file, falling back to template if save doesn't exist
    pub fn load_or_create<P: AsRef<Path>, T: AsRef<Path>>(
        save_path: P,
        template_path: T,
    ) -> anyhow::Result<Self> {
        let save_path = save_path.as_ref();

        if save_path.exists() {
            info!("Loading saved world state from {:?}", save_path);
            Self::load_from_file(save_path)
        } else {
            info!(
                "No save file at {:?}, loading template from {:?}",
                save_path,
                template_path.as_ref()
            );
            Self::load_from_file(template_path)
        }
    }

    /// Save world state to a RON file
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> anyhow::Result<()> {
        let pretty_config = ron::ser::PrettyConfig::new()
            .depth_limit(4)
            .separate_tuple_members(true)
            .enumerate_arrays(true);

        let serialized = ron::ser::to_string_pretty(self, pretty_config)?;
        fs::write(path, serialized)?;
        Ok(())
    }

    /// Get the save file path for a given topic
    pub fn save_path_for_topic(topic: &str) -> String {
        // Sanitize topic name for filesystem
        let safe_topic = topic.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
        format!("data/world_{}.ron", safe_topic)
    }
}
