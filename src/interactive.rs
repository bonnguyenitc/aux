use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use std::io::{self, Write};
use std::time::Duration;

use crate::player::{MediaPlayer, MpvPlayer};
use crate::youtube::types::{format_duration, VideoInfo};

pub async fn run_interactive(
    player: &mut MpvPlayer,
    _video: &VideoInfo,
) -> Result<InteractiveAction> {
    enable_raw_mode()?;
    let result = interactive_loop(player).await;
    disable_raw_mode()?;
    println!(); // newline after raw mode
    result
}

#[derive(Debug)]
pub enum InteractiveAction {
    Quit,
    NewSearch,
    Chat,
    ToggleFavorite,
    AddToQueue,
}

async fn interactive_loop(player: &MpvPlayer) -> Result<InteractiveAction> {
    loop {
        // Render status line
        if let (Ok(pos), Ok(dur)) = (player.get_position().await, player.get_duration().await) {
            let pos_str = format_duration(pos.as_secs());
            let dur_str = format_duration(dur.as_secs());

            // Calculate progress bar
            let progress = if dur.as_secs() > 0 {
                (pos.as_secs() as f64 / dur.as_secs() as f64).min(1.0)
            } else {
                0.0
            };
            let bar_width = 30;
            let filled = (progress * bar_width as f64) as usize;
            let empty = bar_width - filled;
            let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));

            print!("\r  {} {} / {}   ", bar, pos_str, dur_str);
            io::stdout().flush()?;
        }

        // Poll for keyboard events (500ms timeout)
        if event::poll(Duration::from_millis(500))? {
            if let Event::Key(KeyEvent {
                code, modifiers, ..
            }) = event::read()?
            {
                match (code, modifiers) {
                    (KeyCode::Char('q'), _)
                    | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                        return Ok(InteractiveAction::Quit);
                    }
                    (KeyCode::Char(' '), _) => {
                        player.toggle_pause().await.ok();
                    }
                    (KeyCode::Left, _) => {
                        player.seek(-10.0).await.ok();
                    }
                    (KeyCode::Right, _) => {
                        player.seek(10.0).await.ok();
                    }
                    (KeyCode::Up, _) => {
                        if let Ok(vol) = player.get_property_volume().await {
                            let new_vol = (vol + 5).min(100);
                            player.set_volume(new_vol).await.ok();
                        }
                    }
                    (KeyCode::Down, _) => {
                        if let Ok(vol) = player.get_property_volume().await {
                            let new_vol = vol.saturating_sub(5);
                            player.set_volume(new_vol).await.ok();
                        }
                    }
                    (KeyCode::Char('s'), _) => {
                        return Ok(InteractiveAction::NewSearch);
                    }
                    (KeyCode::Char('c'), _) => {
                        return Ok(InteractiveAction::Chat);
                    }
                    (KeyCode::Char('f'), _) => {
                        return Ok(InteractiveAction::ToggleFavorite);
                    }
                    (KeyCode::Char('a'), _) => {
                        return Ok(InteractiveAction::AddToQueue);
                    }
                    _ => {}
                }
            }
        }
    }
}
