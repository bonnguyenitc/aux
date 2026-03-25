use thiserror::Error;

#[derive(Error, Debug)]
// All variants are part of the complete error taxonomy; some are only raised by
// planned daemon/remote features not yet built into this binary.
#[allow(dead_code)]
pub enum AuxError {
    #[error("yt-dlp not found in PATH. Install it: brew install yt-dlp")]
    YtDlpNotFound,

    #[error("mpv not found in PATH. Install it: brew install mpv")]
    MpvNotFound,

    #[error("No search results found for: {query}")]
    NoResults { query: String },

    #[error("Failed to parse yt-dlp output: {0}")]
    ParseError(String),

    #[error("Playback error: {0}")]
    PlaybackError(String),

    #[error("mpv IPC connection failed: {0}")]
    IpcError(String),

    #[error("Config error: {0}")]
    ConfigError(String),

    #[error("yt-dlp command failed: {0}")]
    YtDlpError(String),

    #[error("No active aux session. Start one with: aux play <url>")]
    NoActiveSession,

    #[error("Another aux instance is already running (PID {pid})")]
    AlreadyRunning { pid: u32 },

    #[error("mpv process died unexpectedly. Try: aux play <url>")]
    MpvDied,

    #[error("Invalid speed: {0}. Must be 0.25-4.0")]
    InvalidSpeed(f64),

    #[error("Invalid volume: {0}. Must be 0-100")]
    InvalidVolume(u8),

    #[error("Failed to fetch stream URL for: {title}")]
    StreamFetchFailed { title: String },

    #[error(transparent)]
    Io(#[from] std::io::Error),
}
