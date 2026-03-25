use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    YouTube,
    SoundCloud,
    YTMusic,
    Bandcamp,
    Generic,
}

impl Default for Source {
    fn default() -> Self {
        Source::YouTube
    }
}

impl Source {
    /// Returns the yt-dlp search prefix, or None if search is not supported.
    pub fn search_prefix(&self) -> Option<&'static str> {
        match self {
            Source::YouTube => Some("ytsearch"),
            Source::SoundCloud => Some("scsearch"),
            Source::YTMusic => Some("ytmusicsearch"),
            _ => None,
        }
    }

    /// All sources that support search (for cycling in TUI).
    pub fn searchable() -> &'static [Source] {
        &[Source::YouTube, Source::SoundCloud, Source::YTMusic]
    }

    /// Detect source from yt-dlp's `extractor_key` JSON field.
    pub fn from_extractor_key(key: &str) -> Self {
        match key.to_lowercase().as_str() {
            "youtube" => Source::YouTube,
            "soundcloud" | "soundcloudplaylist" => Source::SoundCloud,
            "bandcamp" | "bandcampalbum" => Source::Bandcamp,
            s if s.contains("youtubemusic") => Source::YTMusic,
            _ => Source::Generic,
        }
    }

    /// Parse from CLI string (e.g. --source youtube).
    pub fn from_str_arg(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "youtube" | "yt" => Some(Source::YouTube),
            "soundcloud" | "sc" => Some(Source::SoundCloud),
            "ytmusic" | "ytm" => Some(Source::YTMusic),
            "bandcamp" | "bc" => Some(Source::Bandcamp),
            _ => None,
        }
    }

    /// Display icon for TUI/CLI.
    pub fn icon(&self) -> &'static str {
        match self {
            Source::YouTube => "▶",
            Source::SoundCloud => "☁",
            Source::YTMusic => "♫",
            Source::Bandcamp => "🎸",
            Source::Generic => "🌐",
        }
    }

    /// Display name for labels.
    pub fn display_name(&self) -> &'static str {
        match self {
            Source::YouTube => "YouTube",
            Source::SoundCloud => "SoundCloud",
            Source::YTMusic => "YT Music",
            Source::Bandcamp => "Bandcamp",
            Source::Generic => "Other",
        }
    }

    /// Serialize to string for DB storage.
    pub fn as_db_str(&self) -> &'static str {
        match self {
            Source::YouTube => "youtube",
            Source::SoundCloud => "soundcloud",
            Source::YTMusic => "ytmusic",
            Source::Bandcamp => "bandcamp",
            Source::Generic => "generic",
        }
    }

    /// Parse from DB string.
    #[allow(dead_code)]
    pub fn from_db_str(s: &str) -> Self {
        match s {
            "youtube" => Source::YouTube,
            "soundcloud" => Source::SoundCloud,
            "ytmusic" => Source::YTMusic,
            "bandcamp" => Source::Bandcamp,
            _ => Source::Generic,
        }
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}
