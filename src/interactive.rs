use anyhow::Result;
use colored::*;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use std::io::{self, Write};
use std::time::Duration;

use crate::ai::transcript::Transcript;
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
    /// Track finished, auto-play next from queue
    PlayNext,
    /// Sleep timer expired
    SleepStop,
}

pub async fn run_interactive(
    player: &mut MpvPlayer,
    video: &VideoInfo,
    db: &Database,
    transcript: Option<&Transcript>,
) -> Result<InteractiveAction> {
    enable_raw_mode()?;
    let result = interactive_loop(player, video, db, transcript).await;
    disable_raw_mode()?;
    // Clear subtitle line if it was printed
    if transcript.is_some() {
        print!("\r{:width$}\r", "", width = 80);
        io::stdout().flush().ok();
    }
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
    transcript: Option<&Transcript>,
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
            let remaining = bar_width.saturating_sub(filled + 1);
            let bar = format!(
                "{}{}{}",
                "━".repeat(filled).green(),
                "●".bold(),
                "─".repeat(remaining).dimmed()
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

            let speed_str = if (speed - 1.0_f64).abs() > 0.01 {
                format!("{}", format!("{}x", speed).yellow())
            } else {
                format!("{}x", speed)
            };

            let eq_icon = crate::player::state::StateFile::read()
                .ok()
                .and_then(|s| s.eq_preset)
                .map(|p| if p != "flat" { format!(" 🎛️{}", p) } else { String::new() })
                .unwrap_or_default();

            print!(
                "\r  {} {} {} {} · 🔊{}% · {}{}{}{}{}{}   ",
                play_icon,
                bar,
                format_duration(pos_s).cyan(),
                format!("/ {}", format_duration(dur_s)).dimmed(),
                vol,
                speed_str,
                eq_icon,
                repeat_icon,
                shuffle_icon,
                fav_icon,
                queue_icon,
            );
            io::stdout().flush()?;
        }

        // ── Render subtitle line (below status bar) ──────────────────────
        if let Some(t) = transcript {
            // Only show for real subtitles, not description fallback
            if t.language != "description" {
                if let Ok(pos) = player.get_position().await {
                    let pos_ms = pos.as_millis() as u64;
                    let seg_text = t.segments.iter().find(|s| {
                        pos_ms >= s.start.as_millis() as u64 && pos_ms < s.end.as_millis() as u64
                    }).map(|s| s.text.as_str()).unwrap_or("");

                    // Truncate to fit one line (terminal width - padding)
                    let max_w = crossterm::terminal::size().map(|(w, _)| w as usize).unwrap_or(80).saturating_sub(8);
                    let display = if seg_text.len() > max_w {
                        &seg_text[..max_w]
                    } else {
                        seg_text
                    };
                    // Print on next line, pad to clear old text, cursor back up
                    print!("\n\r  📝 {:<width$}\x1b[A\r", display.dimmed(), width = max_w);
                    io::stdout().flush()?;
                }
            }
        }

        // ── Sleep timer enforcement ──────────────────────────────────────
        if let Ok(sf) = crate::player::state::StateFile::read() {
            if let Some(deadline) = sf.sleep_deadline {
                if chrono::Utc::now() >= deadline {
                    return Ok(InteractiveAction::SleepStop);
                }
            }
        }

        // ── EOF detection: auto-play next from queue ─────────────────────
        if state.repeat != RepeatMode::One {
            if let Ok(true) = player.is_finished().await {
                return Ok(InteractiveAction::PlayNext);
            }
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
                        let mode = state.cycle_repeat();
                        // Tell mpv to actually loop (RepeatOne = loop-file inf)
                        player.set_loop_file(mode == RepeatMode::One).await.ok();
                    }

                    // ── Shuffle ────────────────────────────────────────
                    (KeyCode::Char('x'), _) => {
                        state.toggle_shuffle();
                        // Icon updates on next status bar tick automatically
                    }

                    // ── Equalizer cycle ─────────────────────────────────
                    (KeyCode::Char('e'), _) => {
                        let presets = ["flat", "bass-boost", "vocal", "treble", "loudness"];
                        let current = crate::player::state::StateFile::read()
                            .ok()
                            .and_then(|s| s.eq_preset)
                            .unwrap_or_else(|| "flat".to_string());
                        let idx = presets.iter().position(|p| *p == current).unwrap_or(0);
                        let next = presets[(idx + 1) % presets.len()];
                        // Save
                        if let Ok(mut sf) = crate::player::state::StateFile::read() {
                            sf.eq_preset = Some(next.to_string());
                            sf.write().ok();
                        }
                        // Apply
                        let filter = crate::eq_preset_filter(next).unwrap_or("");
                        player.set_audio_filter(filter).await.ok();
                        state.flash(format!("🎛️ EQ: {}", next));
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

                    // ── Sleep timer: t (cycle 15m → 30m → 1h → 2h → off)
                    (KeyCode::Char('t'), _) => {
                        use chrono::Utc;
                        if let Ok(mut sf) = crate::player::state::StateFile::read() {
                            let now = Utc::now();
                            let remaining_mins = sf.sleep_deadline
                                .map(|d| (d - now).num_minutes().max(0))
                                .unwrap_or(0);
                            let (next_mins, label) = if remaining_mins == 0 || sf.sleep_deadline.is_none() {
                                (15, "15min")
                            } else if remaining_mins <= 15 {
                                (30, "30min")
                            } else if remaining_mins <= 30 {
                                (60, "1h")
                            } else if remaining_mins <= 60 {
                                (120, "2h")
                            } else {
                                (0, "off")
                            };

                            if next_mins == 0 {
                                sf.sleep_deadline = None;
                                sf.write().ok();
                                state.flash("😴 Sleep timer cancelled".to_string());
                            } else {
                                let deadline = now + chrono::Duration::minutes(next_mins);
                                sf.sleep_deadline = Some(deadline);
                                sf.write().ok();
                                state.flash(format!("😴 Sleep in {} ({})", label, deadline.format("%H:%M")));
                            }
                        }
                    }

                    _ => {}
                }
            }
        }
    }
}
