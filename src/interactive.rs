use anyhow::Result;
use colored::*;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use std::io::{self, Write};
use std::time::Duration;

use crate::library::Database;
use crate::player::{MediaPlayer, MpvPlayer};
use crate::player::types::RepeatMode;
use crate::youtube::types::{format_duration, VideoInfo};
use crate::util::next_speed_preset;

/// Actions that require leaving the interactive loop (need the player stopped or suspended)
#[derive(Debug)]
pub enum InteractiveAction {
    Quit,
    NewSearch,
    Chat,
}

pub async fn run_interactive(
    player: &mut MpvPlayer,
    video: &VideoInfo,
    db: &Database,
) -> Result<InteractiveAction> {
    enable_raw_mode()?;
    let result = interactive_loop(player, video, db).await;
    disable_raw_mode()?;
    println!(); // newline after raw mode
    result
}

// ── Internal state ─────────────────────────────────────────────────────────

struct PlayerState {
    repeat: RepeatMode,
    shuffle: bool,
    /// Whether the current video is in favorites (shown as ❤️ in status bar)
    is_fav: bool,
    /// Whether the current video is in the queue (shown as 📋 in status bar)
    in_queue: bool,
    /// Flash message shown instead of the normal status bar for one tick
    flash: Option<String>,
}

impl PlayerState {
    fn new(is_fav: bool, in_queue: bool) -> Self {
        Self {
            repeat: RepeatMode::Off,
            shuffle: false,
            is_fav,
            in_queue,
            flash: None,
        }
    }

    fn cycle_repeat(&mut self) -> RepeatMode {
        self.repeat = self.repeat.cycle();
        self.repeat
    }

    fn toggle_shuffle(&mut self) -> bool {
        self.shuffle = !self.shuffle;
        self.shuffle
    }

    /// Set a message that replaces the status bar for the next render tick
    fn flash(&mut self, msg: impl Into<String>) {
        self.flash = Some(msg.into());
    }
}

// ── Main loop ───────────────────────────────────────────────────────────────

async fn interactive_loop(
    player: &MpvPlayer,
    video: &VideoInfo,
    db: &Database,
) -> Result<InteractiveAction> {
    // Snapshot fav/queue state once at start (cheap DB reads)
    let is_fav = crate::library::favorites::is_favorite(db, &video.id).unwrap_or(false);
    let in_queue = crate::library::queue::is_in_queue(db, &video.id).unwrap_or(false);
    let mut state = PlayerState::new(is_fav, in_queue);

    loop {
        // ── Render status bar (or flash message) ──────────────────────────
        if let Some(msg) = state.flash.take() {
            // Show feedback in-place, overwriting the progress line
            print!("\r  {:<60}\r", msg);
            io::stdout().flush()?;
        } else if let (Ok(pos), Ok(dur), Ok(vol), Ok(speed), Ok(paused)) = (
            player.get_position().await,
            player.get_duration().await,
            player.get_volume().await,
            player.get_speed().await,
            player.get_paused().await,
        ) {
            let pos_s = pos.as_secs();
            let dur_s = dur.as_secs();
            let progress = if dur_s > 0 {
                (pos_s as f64 / dur_s as f64).min(1.0)
            } else {
                0.0
            };

            let bar_width: usize = 28;
            let filled = (progress * bar_width as f64) as usize;
            let bar = format!(
                "{}{}",
                "█".repeat(filled),
                "░".repeat(bar_width - filled)
            );

            let play_icon = if paused { "⏸" } else { "▶" };
            let repeat_icon = match state.repeat {
                RepeatMode::Off => "",
                RepeatMode::One => " 🔂",
                RepeatMode::All => " 🔁",
            };
            let shuffle_icon = if state.shuffle { " 🔀" } else { "" };
            let fav_icon    = if state.is_fav  { " ❤️" } else { "" };
            let queue_icon  = if state.in_queue { " 📋" } else { "" };

            print!(
                "\r  {} {} {}/{} 🔊{}% ⚡{}x{}{}{}{}   ",
                play_icon,
                bar.green(),
                format_duration(pos_s).cyan(),
                format_duration(dur_s).dimmed(),
                vol,
                speed,
                repeat_icon,
                shuffle_icon,
                fav_icon,
                queue_icon,
            );
            io::stdout().flush()?;
        }

        // ── Poll keyboard events (500ms timeout) ──────────────────────────
        if event::poll(Duration::from_millis(500))? {
            if let Event::Key(KeyEvent {
                code, modifiers, ..
            }) = event::read()?
            {
                match (code, modifiers) {
                    // ── Quit ──────────────────────────────────────────────
                    (KeyCode::Char('q'), _)
                    | (KeyCode::Char('c'), KeyModifiers::CONTROL)
                    | (KeyCode::Esc, _) => {
                        return Ok(InteractiveAction::Quit);
                    }

                    // ── Pause / Resume ─────────────────────────────────
                    (KeyCode::Char(' '), _) => {
                        player.toggle_pause().await.ok();
                    }

                    // ── Seek ±10s ──────────────────────────────────────
                    (KeyCode::Left, KeyModifiers::NONE) => {
                        player.seek(-10.0).await.ok();
                    }
                    (KeyCode::Right, KeyModifiers::NONE) => {
                        player.seek(10.0).await.ok();
                    }

                    // ── Seek ±30s (Shift + arrow) ──────────────────────
                    (KeyCode::Left, KeyModifiers::SHIFT) => {
                        player.seek(-30.0).await.ok();
                    }
                    (KeyCode::Right, KeyModifiers::SHIFT) => {
                        player.seek(30.0).await.ok();
                    }

                    // ── Volume ────────────────────────────────────────
                    (KeyCode::Up, _) => {
                        if let Ok(vol) = player.get_volume().await {
                            player.set_volume((vol + 5).min(100)).await.ok();
                        }
                    }
                    (KeyCode::Down, _) => {
                        if let Ok(vol) = player.get_volume().await {
                            player.set_volume(vol.saturating_sub(5)).await.ok();
                        }
                    }

                    // ── Speed up ──────────────────────────────────────
                    (KeyCode::Char('+'), _) | (KeyCode::Char('='), _) => {
                        if let Ok(spd) = player.get_speed().await {
                            player.set_speed(next_speed_preset(spd, true)).await.ok();
                        }
                    }

                    // ── Speed down ────────────────────────────────────
                    (KeyCode::Char('-'), _) => {
                        if let Ok(spd) = player.get_speed().await {
                            player.set_speed(next_speed_preset(spd, false)).await.ok();
                        }
                    }

                    // ── Repeat cycle ───────────────────────────────────
                    (KeyCode::Char('r'), _) => {
                        state.cycle_repeat();
                        // Icon updates on next status bar tick automatically
                    }

                    // ── Shuffle ────────────────────────────────────────
                    (KeyCode::Char('x'), _) => {
                        state.toggle_shuffle();
                        // Icon updates on next status bar tick automatically
                    }

                    // ── New search ─────────────────────────────────────
                    (KeyCode::Char('s'), _) => {
                        return Ok(InteractiveAction::NewSearch);
                    }

                    // ── Chat ───────────────────────────────────────────
                    (KeyCode::Char('c'), _) => {
                        return Ok(InteractiveAction::Chat);
                    }

                    // ── Toggle favorite — handled in-loop ─────────────
                    (KeyCode::Char('f'), _) => {
                        if state.is_fav {
                            match crate::library::favorites::remove_favorite(db, &video.id) {
                                Ok(_) => {
                                    state.is_fav = false;
                                    state.flash("💔 removed from favorites".to_string());
                                }
                                Err(e) => state.flash(format!("❌ error: {}", e)),
                            }
                        } else {
                            match crate::library::favorites::add_favorite(db, video) {
                                Ok(_) => {
                                    state.is_fav = true;
                                    state.flash("❤️  added to favorites!".to_string());
                                }
                                Err(e) => state.flash(format!("❌ error: {}", e)),
                            }
                        }
                    }

                    // ── Toggle queue — handled in-loop ─────────────────
                    (KeyCode::Char('a'), _) => {
                        if state.in_queue {
                            match crate::library::queue::remove_from_queue_by_video_id(
                                db, &video.id,
                            ) {
                                Ok(_) => {
                                    state.in_queue = false;
                                    state.flash("📋✗ removed from queue".to_string());
                                }
                                Err(e) => state.flash(format!("❌ error: {}", e)),
                            }
                        } else {
                            match crate::library::queue::add_to_queue(db, video) {
                                Ok(_) => {
                                    state.in_queue = true;
                                    let len =
                                        crate::library::queue::queue_length(db).unwrap_or(0);
                                    state.flash(format!("📋 added to queue (#{})", len));
                                }
                                Err(e) => state.flash(format!("❌ queue error: {}", e)),
                            }
                        }
                    }

                    _ => {}
                }
            }
        }
    }
}
