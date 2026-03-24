use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub player: PlayerConfig,
    #[serde(default)]
    pub youtube: YoutubeConfig,
    #[serde(default)]
    pub ai: Option<AiConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PlayerConfig {
    #[serde(default = "default_player_backend")]
    pub backend: String,
    #[serde(default = "default_volume")]
    pub default_volume: u8,
    #[serde(default = "default_search_results")]
    pub search_results: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct YoutubeConfig {
    #[serde(default = "default_youtube_backend")]
    pub backend: String,
    #[serde(default = "default_audio_format")]
    pub prefer_format: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AiConfig {
    #[serde(default = "default_ai_provider")]
    pub provider: String,
    #[serde(default = "default_ai_model")]
    pub model: String,
    #[serde(default = "default_api_key_env")]
    pub api_key_env: String,
    #[serde(default)]
    pub ollama_host: Option<String>,
}

fn default_ai_provider() -> String {
    "openai".to_string()
}
fn default_ai_model() -> String {
    "gpt-4o-mini".to_string()
}
fn default_api_key_env() -> String {
    "OPENAI_API_KEY".to_string()
}

fn default_player_backend() -> String {
    "mpv".to_string()
}
fn default_volume() -> u8 {
    80
}
fn default_search_results() -> usize {
    5
}
fn default_youtube_backend() -> String {
    "yt-dlp".to_string()
}
fn default_audio_format() -> String {
    "m4a".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            player: PlayerConfig::default(),
            youtube: YoutubeConfig::default(),
            ai: None,
        }
    }
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            backend: default_player_backend(),
            default_volume: default_volume(),
            search_results: default_search_results(),
        }
    }
}

impl Default for YoutubeConfig {
    fn default() -> Self {
        Self {
            backend: default_youtube_backend(),
            prefer_format: default_audio_format(),
        }
    }
}

impl Config {
    pub fn config_dir() -> Option<PathBuf> {
        ProjectDirs::from("", "", "duet").map(|d| d.config_dir().to_path_buf())
    }

    pub fn config_path() -> Option<PathBuf> {
        Self::config_dir().map(|d| d.join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        let path = match Self::config_path() {
            Some(p) => p,
            None => return Ok(Self::default()),
        };

        if !path.exists() {
            let config = Self::default();
            config.save()?;
            return Ok(config);
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config: {}", path.display()))?;

        let config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config: {}", path.display()))?;

        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = match Self::config_path() {
            Some(p) => p,
            None => return Ok(()),
        };

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content =
            toml::to_string_pretty(self).context("Failed to serialize config")?;

        fs::write(&path, content)
            .with_context(|| format!("Failed to write config: {}", path.display()))?;

        Ok(())
    }
}
