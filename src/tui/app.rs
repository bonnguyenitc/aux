use crate::ai::chat::ChatMessage;
use crate::ai::transcript::Transcript;
use crate::library::favorites::FavoriteEntry;
use crate::library::playlist::Playlist;
use crate::media::{MediaInfo, Source};
use crate::player::types::RepeatMode;
use chrono::{DateTime, Utc};

/// Active body panel shown in the TUI
#[derive(Debug, Clone, PartialEq)]
pub enum Panel {
    Search,
    Results,
    Lyrics,
    Queue,
    Favorites,
    History,
    Playlists,
    Chat,
    Help,
}

/// Alias kept for backward compatibility with existing match arms in main.rs
#[allow(dead_code)]
pub type AppMode = Panel;

/// Playback state mirrored from mpv + state file
#[derive(Debug, Clone)]
pub struct NowPlaying {
    pub video: MediaInfo,
    pub position_secs: u64,
    pub duration_secs: u64,
    pub paused: bool,
    pub volume: u8,
    pub speed: f64,
    pub repeat: RepeatMode,
    pub shuffle: bool,
    pub is_fav: bool,
    /// Whether the current video is in the play queue
    pub in_queue: bool,
    pub sleep_deadline: Option<DateTime<Utc>>,
    pub eq_preset: String,
}

/// Application state for the TUI
pub struct App {
    pub panel: Panel,
    pub search_input: String,
    /// Full cached search results from yt-dlp (all pages)
    pub all_search_results: Vec<MediaInfo>,
    /// Current page slice — computed from `all_search_results`
    pub search_results: Vec<MediaInfo>,
    /// Current page index (0-based)
    pub search_page: usize,
    /// Items per page
    pub search_page_size: usize,
    pub selected_index: usize,
    pub now_playing: Option<NowPlaying>,
    pub status_message: Option<String>,
    pub should_quit: bool,
    pub queue_items: Vec<crate::library::queue::QueueEntry>,
    pub history_items: Vec<crate::library::history::HistoryEntry>,
    /// Saved search queries (newest first), used for ↑/↓ recall in Search panel
    pub search_history: Vec<String>,
    /// Current position within search_history during ↑/↓ navigation.
    /// `None` means user is editing a fresh query (not navigating history).
    pub search_history_index: Option<usize>,
    /// Stash of user's in-progress input before they started navigating history
    pub search_input_stash: String,
    /// Backwards-compat shim: some code reads app.mode
    pub mode: Panel,
    // ── Chat state ───────────────────────────────────────────────
    /// Current chat input text
    pub chat_input: String,
    /// Chat message history displayed in the chat panel
    pub chat_messages: Vec<ChatMessage>,
    /// Scroll offset for the chat message area (0 = bottom)
    pub chat_scroll: u16,
    /// Whether we are waiting for an AI response
    pub chat_loading: bool,
    // ── Favorites state ──────────────────────────────────────────
    pub fav_items: Vec<FavoriteEntry>,
    // ── Lyrics state ─────────────────────────────────────────────
    /// Transcript for the currently playing track
    pub transcript: Option<Transcript>,
    /// Scroll offset for lyrics panel (0 = auto-scroll to current segment)
    pub lyrics_scroll: u16,
    /// Whether lyrics auto-scrolls to current segment
    pub lyrics_auto_scroll: bool,
    // ── Playlists state ──────────────────────────────────────────
    pub playlist_list: Vec<crate::library::playlist::Playlist>,
    pub playlist_items_view: Option<(String, Vec<crate::library::playlist::PlaylistItem>)>,
    /// When Some, the user is typing a new playlist name
    pub playlist_name_input: Option<String>,
    // ── Playlist picker ("add to playlist" popup) ────────────────
    pub playlist_picker: Option<PlaylistPicker>,
    // ── Help scroll ─────────────────────────────────────────
    pub help_scroll: u16,
    // ── Saved playback positions (for UX display) ───────────
    pub saved_positions: std::collections::HashMap<String, u64>,
    // ── Pending resume seek (deferred until mpv loads stream) ──
    pub pending_resume_seek: Option<u64>,
    // ── Search source ────────────────────────────────────────────
    /// Active search source for TUI search (cycle with Ctrl+S)
    pub search_source: Source,
}

/// Popup state: user is choosing which playlist to add a video to
#[derive(Debug, Clone)]
pub struct PlaylistPicker {
    /// The video to add
    pub video: MediaInfo,
    /// Available playlists to pick from
    pub playlists: Vec<Playlist>,
    /// Currently highlighted index
    pub selected: usize,
}

impl App {
    pub fn new() -> Self {
        Self {
            panel: Panel::Search,
            mode: Panel::Search,
            search_input: String::new(),
            all_search_results: Vec::new(),
            search_results: Vec::new(),
            search_page: 0,
            search_page_size: 10,
            selected_index: 0,
            now_playing: None,
            status_message: None,
            should_quit: false,
            queue_items: Vec::new(),
            history_items: Vec::new(),
            search_history: Vec::new(),
            search_history_index: None,
            search_input_stash: String::new(),
            chat_input: String::new(),
            chat_messages: Vec::new(),
            chat_scroll: 0,
            chat_loading: false,
            fav_items: Vec::new(),
            transcript: None,
            lyrics_scroll: 0,
            lyrics_auto_scroll: true,
            playlist_list: Vec::new(),
            playlist_items_view: None,
            playlist_name_input: None,
            playlist_picker: None,
            help_scroll: 0,
            saved_positions: std::collections::HashMap::new(),
            pending_resume_seek: None,
            search_source: Source::YouTube,
        }
    }

    /// Add a chat message and reset scroll to bottom
    pub fn push_chat_message(&mut self, role: &str, content: &str) {
        self.chat_messages.push(ChatMessage {
            role: role.to_string(),
            content: content.to_string(),
        });
        self.chat_scroll = 0; // auto-scroll to bottom
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_message = Some(msg.into());
    }

    pub fn set_panel(&mut self, panel: Panel) {
        self.selected_index = 0;
        self.mode = panel.clone();
        self.panel = panel;
        // Cancel any in-progress history navigation on panel switch
        self.cancel_search_history_nav();
    }

    /// Press ↑ in the Search panel: go back in search history.
    /// Stashes the current in-progress input the first time so ↓ can restore it.
    pub fn search_history_up(&mut self) {
        if self.search_history.is_empty() {
            return;
        }
        let next_idx = match self.search_history_index {
            None => {
                // First time: stash the live input
                self.search_input_stash = self.search_input.clone();
                0
            }
            Some(i) if i + 1 < self.search_history.len() => i + 1,
            Some(i) => i, // Already at oldest
        };
        self.search_history_index = Some(next_idx);
        self.search_input = self.search_history[next_idx].clone();
    }

    /// Press ↓ in the Search panel: go forward in history, or restore live input.
    pub fn search_history_down(&mut self) {
        match self.search_history_index {
            None => {} // Not navigating, nothing to do
            Some(0) => {
                // Back to the stashed live input
                self.search_history_index = None;
                self.search_input = self.search_input_stash.clone();
            }
            Some(i) => {
                let next = i - 1;
                self.search_history_index = Some(next);
                self.search_input = self.search_history[next].clone();
            }
        }
    }

    /// Reset history navigation (called when user types or presses Enter)
    pub fn cancel_search_history_nav(&mut self) {
        self.search_history_index = None;
        self.search_input_stash.clear();
    }

    pub fn select_next(&mut self) {
        let max = match self.panel {
            Panel::Results => self.search_results.len(),
            Panel::Queue => self.queue_items.len(),
            Panel::Favorites => self.fav_items.len(),
            Panel::History => self.history_items.len(),
            Panel::Playlists => {
                if let Some((_, ref items)) = self.playlist_items_view {
                    items.len()
                } else {
                    self.playlist_list.len()
                }
            }
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

    // ── Pagination helpers ────────────────────────────────────────────

    /// Total number of search result pages
    pub fn search_total_pages(&self) -> usize {
        let total = self.all_search_results.len();
        if total == 0 {
            return 1;
        }
        (total + self.search_page_size - 1) / self.search_page_size
    }

    /// Set search results from a yt-dlp batch and show the first page
    pub fn set_search_results(&mut self, results: Vec<MediaInfo>) {
        self.all_search_results = results;
        self.search_page = 0;
        self.refresh_search_page();
    }

    /// Advance to the next page of search results
    pub fn search_next_page(&mut self) {
        if self.search_page + 1 < self.search_total_pages() {
            self.search_page += 1;
            self.refresh_search_page();
        }
    }

    /// Go to the previous page of search results
    pub fn search_prev_page(&mut self) {
        if self.search_page > 0 {
            self.search_page -= 1;
            self.refresh_search_page();
        }
    }

    /// Recompute `search_results` from the current page of `all_search_results`
    fn refresh_search_page(&mut self) {
        let start = self.search_page * self.search_page_size;
        let end = (start + self.search_page_size).min(self.all_search_results.len());
        self.search_results = self.all_search_results[start..end].to_vec();
        self.selected_index = 0;
    }

    /// Convert a page-local index to a global index into `all_search_results`
    pub fn search_global_index(&self, local_idx: usize) -> usize {
        self.search_page * self.search_page_size + local_idx
    }

    pub fn update_playback(&mut self, position: u64, duration: u64, paused: bool, volume: u8) {
        if let Some(ref mut np) = self.now_playing {
            np.position_secs = position;
            np.duration_secs = duration;
            np.paused = paused;
            np.volume = volume;
        }
    }

    /// Update speed/repeat/shuffle/sleep from state file (called on tick)
    pub fn update_player_meta(
        &mut self,
        speed: f64,
        repeat: RepeatMode,
        shuffle: bool,
        sleep_deadline: Option<DateTime<Utc>>,
        eq_preset: String,
    ) {
        if let Some(ref mut np) = self.now_playing {
            np.speed = speed;
            np.repeat = repeat;
            np.shuffle = shuffle;
            np.sleep_deadline = sleep_deadline;
            np.eq_preset = eq_preset;
        }
    }

    // ── Playlist picker helpers ─────────────────────────────────────

    /// Open the "add to playlist" popup for a given video.
    pub fn open_playlist_picker(&mut self, video: MediaInfo, playlists: Vec<Playlist>) {
        self.playlist_picker = Some(PlaylistPicker {
            video,
            playlists,
            selected: 0,
        });
    }

    /// Move selection up in the picker
    pub fn picker_prev(&mut self) {
        if let Some(ref mut pk) = self.playlist_picker {
            if pk.selected > 0 {
                pk.selected -= 1;
            }
        }
    }

    /// Move selection down in the picker
    pub fn picker_next(&mut self) {
        if let Some(ref mut pk) = self.playlist_picker {
            if pk.selected + 1 < pk.playlists.len() {
                pk.selected += 1;
            }
        }
    }

    /// Close the picker popup
    pub fn close_playlist_picker(&mut self) {
        self.playlist_picker = None;
    }

    /// Cycle through searchable sources (YouTube → SoundCloud → YT Music → …)
    pub fn cycle_search_source(&mut self) {
        let searchable = Source::searchable();
        if searchable.is_empty() {
            return;
        }
        let current_pos = searchable
            .iter()
            .position(|s| s == &self.search_source)
            .unwrap_or(0);
        let next_pos = (current_pos + 1) % searchable.len();
        self.search_source = searchable[next_pos].clone();
        self.set_status(format!(
            "Search source: {}",
            self.search_source.display_name()
        ));
    }
}
