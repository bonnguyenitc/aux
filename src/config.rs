use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub profiles: HashMap<String, AiProfile>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AiProfile {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedAiConfig {
    pub provider: String,
    pub model: String,
    pub api_key: Option<String>,
    pub base_url: String,
}

fn default_ai_provider() -> String {
    "openai".to_string()
}
fn default_ai_model() -> String {
    "gpt-4o-mini".to_string()
}

pub fn default_base_url(provider: &str) -> String {
    match provider {
        "openai" => "https://api.openai.com/v1".to_string(),
        "anthropic" => "https://api.anthropic.com".to_string(),
        "gemini" => "https://generativelanguage.googleapis.com".to_string(),
        "ollama" => "http://localhost:11434".to_string(),
        _ => String::new(),
    }
}

impl AiConfig {
    /// Resolve a profile (or default) into a flat, usable config
    pub fn resolve(&self, profile_name: Option<&str>) -> anyhow::Result<ResolvedAiConfig> {
        let (provider, model, api_key, api_key_env, base_url) = match profile_name {
            Some(name) => {
                let profile = self.profiles.get(name).with_context(|| {
                    let available: Vec<&str> =
                        self.profiles.keys().map(|s| s.as_str()).collect();
                    if available.is_empty() {
                        format!("Profile '{}' not found. No profiles configured.", name)
                    } else {
                        format!(
                            "Profile '{}' not found. Available: {}",
                            name,
                            available.join(", ")
                        )
                    }
                })?;
                (
                    profile.provider.as_deref().unwrap_or(&self.provider),
                    profile.model.as_deref().unwrap_or(&self.model),
                    profile.api_key.as_ref().or(self.api_key.as_ref()),
                    profile.api_key_env.as_ref().or(self.api_key_env.as_ref()),
                    profile.base_url.as_ref().or(self.base_url.as_ref()),
                )
            }
            None => (
                self.provider.as_str(),
                self.model.as_str(),
                self.api_key.as_ref(),
                self.api_key_env.as_ref(),
                self.base_url.as_ref(),
            ),
        };

        // Resolve API key: config value → env var → None
        let resolved_key = api_key.cloned().filter(|k| !k.is_empty()).or_else(|| {
            api_key_env
                .and_then(|var| std::env::var(var).ok().filter(|v| !v.is_empty()))
        });

        // Resolve base_url: explicit → provider default
        let resolved_url = base_url
            .cloned()
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| default_base_url(provider));

        Ok(ResolvedAiConfig {
            provider: provider.to_string(),
            model: model.to_string(),
            api_key: resolved_key,
            base_url: resolved_url,
        })
    }
}

/// Resolve with CLI flag overrides
pub fn resolve_with_overrides(
    ai: &AiConfig,
    profile: Option<&str>,
    model_override: Option<&str>,
) -> anyhow::Result<ResolvedAiConfig> {
    let mut resolved = ai.resolve(profile)?;
    if let Some(m) = model_override {
        resolved.model = m.to_string();
    }
    Ok(resolved)
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
