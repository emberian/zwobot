use super::state::WorldState;
use std::fs;
use std::path::Path;

impl WorldState {
    /// Load world state from a RON file
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let contents = fs::read_to_string(path)?;
        let world: WorldState = ron::from_str(&contents)?;
        Ok(world)
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
}
