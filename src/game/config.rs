use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use super::input::{GameAction, KeyBind, default_bindings};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameConfig {
    pub key_bindings: HashMap<GameAction, KeyBind>,
    pub graphics: GraphicsConfig,
    pub gameplay: GameplayConfig,
    #[serde(default)]
    pub debug: DebugConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphicsConfig {
    pub render_distance: u32,
    pub frame_rate_cap: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameplayConfig {
    pub tiling_n: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DebugConfig {
    pub log_clicks: bool,
    /// Place any structure for free, ignoring inventory.
    #[serde(default)]
    pub free_placement: bool,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            key_bindings: default_bindings(),
            graphics: GraphicsConfig {
                render_distance: 3,
                frame_rate_cap: 60,
            },
            gameplay: GameplayConfig {
                tiling_n: 5,
            },
            debug: DebugConfig::default(),
        }
    }
}

fn config_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "octofact")
        .map(|dirs| dirs.config_dir().join("settings.toml"))
}

impl GameConfig {
    pub fn load() -> Self {
        let Some(path) = config_path() else {
            return Self::default();
        };

        match std::fs::read_to_string(&path) {
            Ok(contents) => {
                match toml::from_str(&contents) {
                    Ok(config) => config,
                    Err(e) => {
                        log::warn!("Failed to parse config: {e}. Using defaults.");
                        Self::default()
                    }
                }
            }
            Err(_) => {
                let config = Self::default();
                config.save();
                config
            }
        }
    }

    pub fn save(&self) {
        let Some(path) = config_path() else {
            log::warn!("Could not determine config directory");
            return;
        };

        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                log::warn!("Failed to create config directory: {e}");
                return;
            }
        }

        match toml::to_string_pretty(self) {
            Ok(contents) => {
                if let Err(e) = std::fs::write(&path, contents) {
                    log::warn!("Failed to write config: {e}");
                }
            }
            Err(e) => {
                log::warn!("Failed to serialize config: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = GameConfig::default();
        assert_eq!(config.graphics.render_distance, 3);
        assert_eq!(config.graphics.frame_rate_cap, 60);
        assert_eq!(config.gameplay.tiling_n, 5);
        assert!(!config.key_bindings.is_empty());
    }

    #[test]
    fn test_config_toml_roundtrip() {
        let config = GameConfig::default();
        let serialized = toml::to_string_pretty(&config).expect("serialize");
        let deserialized: GameConfig = toml::from_str(&serialized).expect("deserialize");
        assert_eq!(deserialized.graphics.render_distance, config.graphics.render_distance);
        assert_eq!(deserialized.graphics.frame_rate_cap, config.graphics.frame_rate_cap);
        assert_eq!(deserialized.gameplay.tiling_n, config.gameplay.tiling_n);
        assert_eq!(deserialized.key_bindings.len(), config.key_bindings.len());
    }
}
