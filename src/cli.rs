use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "duet",
    about = "🎵 CLI YouTube player with AI companion",
    version,
    author
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Search YouTube for videos
    Search {
        /// Search query
        #[arg(required = true)]
        query: Vec<String>,

        /// Number of results to show
        #[arg(short = 'n', long, default_value = "5")]
        limit: usize,
    },

    /// Play a YouTube video (audio only)
    Play {
        /// YouTube URL or video ID
        url: String,
    },

    /// Show what's currently playing
    Now,

    /// Pause playback
    Pause,

    /// Resume playback
    Resume,

    /// Stop playback
    Stop,

    /// Set volume (0-100)
    Volume {
        /// Volume level (0-100)
        level: u8,
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

    /// Manage duet configuration
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
    /// Configure YouTube settings
    Youtube {
        #[command(subcommand)]
        action: Option<YoutubeAction>,
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
pub enum YoutubeAction {
    /// Set YouTube settings
    Set {
        /// Preferred audio format (e.g. "m4a", "opus", "webm")
        #[arg(long)]
        format: Option<String>,

        /// YouTube backend (e.g. "yt-dlp")
        #[arg(long)]
        backend: Option<String>,
    },
}
