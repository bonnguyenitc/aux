use super::source::Source;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MediaInfo {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub view_count: Option<u64>,
    #[serde(default)]
    pub thumbnail: Option<String>,
    pub url: String,
    /// Video description — used as fallback when no subtitles are available.
    #[serde(default)]
    pub description: Option<String>,
    /// Detected from yt-dlp extractor_key, populated after deserialization.
    #[serde(skip)]
    pub source: Source,
    /// Raw extractor key from yt-dlp JSON.
    #[serde(default)]
    pub extractor_key: Option<String>,
}

impl MediaInfo {
    /// Post-deserialization: populate `source` from `extractor_key`.
    pub fn resolve_source(&mut self) {
        if let Some(ref key) = self.extractor_key {
            self.source = Source::from_extractor_key(key);
        }
    }
}

impl fmt::Display for MediaInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let channel = self.channel.as_deref().unwrap_or("Unknown");
        let duration = self
            .duration
            .map(|d| format_duration(d as u64))
            .unwrap_or_else(|| "LIVE".to_string());

        write!(
            f,
            "{} {} — {} [{}]",
            self.source.icon(),
            self.title,
            channel,
            duration
        )
    }
}

#[derive(Debug, Clone)]
pub struct StreamUrl {
    pub audio_url: String,
    #[allow(dead_code)] // Stored for future format/quality selection
    pub format: String,
}

pub fn format_duration(total_secs: u64) -> String {
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    if hours > 0 {
        format!("{:}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{:}:{:02}", minutes, seconds)
    }
}
