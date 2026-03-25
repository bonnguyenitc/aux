use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "aux",
    about = "🎵 Listen music with AI agent",
    version,
    author
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Search for audio
    Search {
        /// Search query
        #[arg(required = true)]
        query: Vec<String>,

        /// Number of results to show
        #[arg(short = 'n', long, default_value = "5")]
        limit: usize,

        /// Search source (youtube, soundcloud, ytmusic)
        #[arg(long, default_value = "youtube")]
        source: String,
    },

    /// Play audio from a URL or search query
    Play {
        /// URL or search query
        url: String,
        /// Run in background (daemon mode)
        #[arg(long, short = 'd')]
        daemon: bool,
        /// Initial playback speed (0.25-4.0)
        #[arg(long)]
        speed: Option<f64>,
        /// Initial repeat mode
        #[arg(long)]
        repeat: Option<RepeatArg>,
    },

    /// Show what's currently playing
    Now {
        /// Output format
        #[arg(long, default_value = "pretty")]
        format: OutputFormat,
    },

    /// Pause playback
    Pause,

    /// Resume playback
    Resume,

    /// Stop playback and quit daemon
    Stop,

    /// Set or show volume (0-100)
    Volume {
        /// Volume level 0-100 (omit to show current)
        level: Option<u8>,
    },

    /// Seek playback position (+10, -30, 2:30, 1:02:30)
    Seek {
        /// Offset (+10, -30) or absolute position (2:30, 1:02:30)
        position: String,
    },

    /// Skip to next track in queue
    Next,

    /// Go to previous track
    Prev,

    /// Set or cycle playback speed (0.25-4.0, 'up', 'down')
    Speed {
        /// Speed value or 'up'/'down' (omit to show current)
        value: Option<String>,
    },

    /// Set or cycle repeat mode
    Repeat {
        /// Repeat mode: off | one | all (omit to cycle)
        mode: Option<RepeatArg>,
    },

    /// Toggle shuffle
    Shuffle,

    /// Set sleep timer (30m, 1h, 1h30m, off)
    Sleep {
        /// Duration or 'off' to cancel
        duration: String,
    },

    /// Show listening stats
    Stats {
        /// Time range: today | week | month | all
        #[arg(default_value = "all")]
        range: String,
    },

    /// Show daemon logs
    Logs {
        /// Number of lines to show
        #[arg(short = 'n', long, default_value = "50")]
        lines: usize,
        /// Follow log output
        #[arg(long)]
        follow: bool,
    },

    /// Chat with AI about the current video
    Chat {
        /// Message to send (or empty for interactive chat)
        message: Vec<String>,

        /// Override AI model for this session
        #[arg(long)]
        model: Option<String>,

        /// Use a named AI profile
        #[arg(long)]
        profile: Option<String>,
    },

    /// Ask AI to suggest related videos
    Suggest {
        /// Override AI model for this session
        #[arg(long)]
        model: Option<String>,

        /// Use a named AI profile
        #[arg(long)]
        profile: Option<String>,
    },

    /// Show play history
    History {
        /// Number of entries to show
        #[arg(short = 'n', long, default_value = "20")]
        limit: usize,

        /// Show today's history only
        #[arg(long)]
        today: bool,
    },

    /// Manage favorites
    #[command(alias = "fav")]
    Favorites {
        #[command(subcommand)]
        action: Option<FavAction>,
    },

    /// Manage play queue
    #[command(alias = "q")]
    Queue {
        #[command(subcommand)]
        action: Option<QueueAction>,
    },

    /// Manage playlists
    #[command(alias = "pl")]
    Playlist {
        #[command(subcommand)]
        action: Option<PlaylistAction>,
    },

    /// Set equalizer preset
    #[command(alias = "eq")]
    Equalizer {
        /// Preset: flat, bass-boost, vocal, treble, loudness (omit to show current)
        preset: Option<String>,
    },

    /// Manage aux configuration
    #[command(alias = "cfg")]
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
}

#[derive(Subcommand, Debug)]
pub enum FavAction {
    /// List all favorites
    List,
    /// Add a video URL to favorites
    Add {
        /// YouTube URL
        url: String,
    },
    /// Remove a video from favorites
    Remove {
        /// Video ID
        video_id: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum QueueAction {
    /// Show current queue
    List,
    /// Add a video URL to queue
    Add {
        /// YouTube URL
        url: String,
    },
    /// Play next video in queue
    Next,
    /// Clear the queue
    Clear,
}

#[derive(Subcommand, Debug)]
pub enum PlaylistAction {
    /// Create a new playlist
    Create {
        /// Playlist name
        name: String,
    },
    /// List all playlists
    List,
    /// Show items in a playlist
    Show {
        /// Playlist name
        name: String,
    },
    /// Add a video URL to a playlist
    Add {
        /// Playlist name
        name: String,
        /// YouTube URL
        url: String,
    },
    /// Remove a video from a playlist
    Remove {
        /// Playlist name
        name: String,
        /// Video ID
        video_id: String,
    },
    /// Play a playlist (load all items into queue and start)
    Play {
        /// Playlist name
        name: String,
    },
    /// Delete a playlist
    Delete {
        /// Playlist name
        name: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Configure AI companion settings
    Ai {
        #[command(subcommand)]
        action: Option<AiAction>,

        /// Run interactive AI setup wizard
        #[arg(long)]
        setup: bool,
    },
    /// Configure player settings
    Player {
        #[command(subcommand)]
        action: Option<PlayerAction>,
    },
    /// Configure media source settings
    #[command(alias = "youtube")]
    Media {
        #[command(subcommand)]
        action: Option<MediaAction>,
    },
    /// Set a config key (e.g. ai.provider, player.default_volume)
    Set {
        /// Config key
        key: String,
        /// Value to set
        value: String,
    },
    /// Get a config key value
    Get {
        /// Config key
        key: String,
    },
    /// Print path to config file
    Path,
    /// Reset config to defaults
    Reset {
        /// Skip confirmation prompt
        #[arg(long, short = 'f')]
        force: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum AiAction {
    /// Set default AI configuration
    Set {
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        api_key: Option<String>,
        #[arg(long)]
        api_key_env: Option<String>,
        #[arg(long)]
        base_url: Option<String>,
    },
    /// Add or update a named profile
    AddProfile {
        /// Profile name (e.g. "deep", "local", "groq")
        name: String,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        api_key: Option<String>,
        #[arg(long)]
        api_key_env: Option<String>,
        #[arg(long)]
        base_url: Option<String>,
    },
    /// Remove a named profile
    RemoveProfile {
        /// Profile name to remove
        name: String,
    },
    /// List all AI profiles
    ListProfiles,
    /// Test AI connection
    Test {
        /// Test a specific profile instead of default
        #[arg(long)]
        profile: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum PlayerAction {
    /// Set player settings
    Set {
        /// Default playback volume (0-100)
        #[arg(long)]
        volume: Option<u8>,

        /// Number of search results to show
        #[arg(long)]
        search_results: Option<usize>,

        /// Player backend (e.g. "mpv")
        #[arg(long)]
        backend: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum MediaAction {
    /// Set media source settings
    Set {
        /// Preferred audio format (e.g. "m4a", "opus", "webm")
        #[arg(long)]
        format: Option<String>,

        /// Media backend (e.g. "yt-dlp")
        #[arg(long)]
        backend: Option<String>,

        /// Default search source (youtube, soundcloud, ytmusic)
        #[arg(long)]
        default_source: Option<String>,
    },
}
#[derive(Debug, Clone, clap::ValueEnum)]
pub enum RepeatArg {
    Off,
    One,
    All,
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum OutputFormat {
    Pretty,
    Json,
    Oneline,
}
