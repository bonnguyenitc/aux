use thiserror::Error;

#[derive(Error, Debug)]
pub enum DuetError {
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

    #[error(transparent)]
    Io(#[from] std::io::Error),
}
