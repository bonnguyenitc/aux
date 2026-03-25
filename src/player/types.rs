use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::media::MediaInfo;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum RepeatMode {
    #[serde(rename = "off")]
    Off,
    #[serde(rename = "one")]
    One,
    #[serde(rename = "all")]
    All,
}

impl RepeatMode {
    pub fn cycle(self) -> Self {
        match self {
            Self::Off => Self::One,
            Self::One => Self::All,
            Self::All => Self::Off,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "🔁 off",
            Self::One => "🔂 one",
            Self::All => "🔁 all",
        }
    }
}

impl Default for RepeatMode {
    fn default() -> Self {
        Self::Off
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NowPlayingInfo {
    pub video: MediaInfo,
    pub position_secs: f64,
    pub duration_secs: f64,
    pub volume: u8,
    pub speed: f64,
    pub paused: bool,
    pub repeat: RepeatMode,
    pub shuffle: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sleep_deadline: Option<DateTime<Utc>>,
    pub eq_preset: String,
}

/// IPC action dispatch type — planned for future daemon event loop.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum PlayerAction {
    TogglePause,
    Pause,
    Resume,
    Stop,
    Next,
    Prev,
    SeekRelative(f64),
    SeekAbsolute(f64),
    SpeedUp,
    SpeedDown,
    SetSpeed(f64),
    CycleRepeat,
    ToggleShuffle,
    VolumeUp,
    VolumeDown,
    SetVolume(u8),
    ToggleFavorite,
    AddToQueue,
    NewSearch,
    Chat,
    Quit,
}
