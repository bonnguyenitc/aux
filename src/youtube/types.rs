use serde::Deserialize;
use std::fmt;

#[derive(Debug, Clone, Deserialize)]
pub struct VideoInfo {
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
}

impl fmt::Display for VideoInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let channel = self.channel.as_deref().unwrap_or("Unknown");
        let duration = self
            .duration
            .map(|d| format_duration(d as u64))
            .unwrap_or_else(|| "LIVE".to_string());

        write!(f, "{} — {} [{}]", self.title, channel, duration)
    }
}

#[derive(Debug, Clone)]
pub struct StreamUrl {
    pub audio_url: String,
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
