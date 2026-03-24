use crate::library::Database;
use crate::youtube::VideoInfo;

/// Application state for the TUI
pub struct App {
    pub mode: AppMode,
    pub search_input: String,
    pub search_results: Vec<VideoInfo>,
    pub selected_index: usize,
    pub now_playing: Option<NowPlaying>,
    pub status_message: Option<String>,
    pub should_quit: bool,
}

pub struct NowPlaying {
    pub video: VideoInfo,
    pub position_secs: u64,
    pub duration_secs: u64,
    pub paused: bool,
    pub volume: u8,
    pub is_fav: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    Search,
    Results,
    Playing,
    History,
    Favorites,
    Queue,
    Help,
}

impl App {
    pub fn new() -> Self {
        Self {
            mode: AppMode::Search,
            search_input: String::new(),
            search_results: Vec::new(),
            selected_index: 0,
            now_playing: None,
            status_message: None,
            should_quit: false,
        }
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_message = Some(msg.into());
    }

    pub fn select_next(&mut self) {
        let max = match self.mode {
            AppMode::Results => self.search_results.len(),
            _ => 0,
        };
        if max > 0 && self.selected_index < max - 1 {
            self.selected_index += 1;
        }
    }

    pub fn select_prev(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn update_playback(&mut self, position: u64, duration: u64, paused: bool, volume: u8) {
        if let Some(ref mut np) = self.now_playing {
            np.position_secs = position;
            np.duration_secs = duration;
            np.paused = paused;
            np.volume = volume;
        }
    }
}
