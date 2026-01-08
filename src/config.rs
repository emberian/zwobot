use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub channel: String,
    #[serde(default = "default_world_data_path")]
    pub world_data_path: String,
    pub topics: HashMap<String, TopicConfig>,
    pub default_topic: TopicConfig,
}

fn default_world_data_path() -> String {
    "data/world_state.ron".to_string()
}

#[derive(Debug, Deserialize, Clone)]
pub struct TopicConfig {
    pub model_id: String,
    pub quantization: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default)]
    pub characters: Vec<Character>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Character {
    pub name: String,
    #[serde(default)]
    pub system_prompt: String,
}

fn default_max_tokens() -> u32 {
    1024
}

fn default_temperature() -> f32 {
    0.7
}

impl TopicConfig {
    /// Get characters for this topic, or a default assistant if none configured
    pub fn get_characters(&self) -> Vec<Character> {
        if self.characters.is_empty() {
            vec![Character {
                name: "Assistant".to_string(),
                system_prompt: "You are a helpful AI assistant participating in a conversation.".to_string(),
            }]
        } else {
            self.characters.clone()
        }
    }
}

impl AppConfig {
    pub fn load() -> anyhow::Result<Self> {
        let config_path = Path::new("config/default.toml");
        let config_str = fs::read_to_string(config_path)?;
        let config: AppConfig = toml::from_str(&config_str)?;
        Ok(config)
    }

    /// Get the topic configuration for a given topic name.
    /// Returns the default config if the topic is not found.
    pub fn get_topic_config(&self, topic: &str) -> &TopicConfig {
        self.topics.get(topic).unwrap_or(&self.default_topic)
    }
}

#[derive(Debug, Deserialize)]
pub struct ZulipConfig {
    pub email: String,
    pub key: String,
    pub site: String,
}

impl ZulipConfig {
    pub fn load() -> anyhow::Result<Self> {
        let zuliprc_path = Path::new("zuliprc");
        let content = fs::read_to_string(zuliprc_path)?;

        let mut email = String::new();
        let mut key = String::new();
        let mut site = String::new();

        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("email=") {
                email = line.strip_prefix("email=").unwrap().to_string();
            } else if line.starts_with("key=") {
                key = line.strip_prefix("key=").unwrap().to_string();
            } else if line.starts_with("site=") {
                site = line.strip_prefix("site=").unwrap().to_string();
            }
        }

        Ok(ZulipConfig { email, key, site })
    }
}
