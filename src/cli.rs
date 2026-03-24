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
    },

    /// Ask AI to suggest related videos
    Suggest,

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
