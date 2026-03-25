use anyhow::{Context, Result};
use colored::Colorize;

use crate::player::remote::RemoteSession;
use crate::player::state::StateFile;
use crate::player::types::RepeatMode;
use crate::player::MediaPlayer;
use crate::util::{next_speed_preset, parse_duration_str, parse_timestamp};
use crate::youtube::types::format_duration;

pub async fn cmd_now(format: &str) -> Result<()> {
    let remote =
        RemoteSession::connect().context("No active duet session. Start one with: duet play <url>")?;
    let info = remote.now().await?;

    let status = if info.paused { "⏸ Paused" } else { "▶ Playing" };
    let pos = format_duration(info.position_secs as u64);
    let dur = format_duration(info.duration_secs as u64);
    let ch = info.video.channel.as_deref().unwrap_or("Unknown");

    match format {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&info).unwrap_or_default());
        }
        "oneline" => {
            let s = if info.paused { "⏸" } else { "▶" };
            let eq = if info.eq_preset != "flat" { format!(" 🎛️{}", info.eq_preset) } else { String::new() };
            println!(
                "{} {} — {} [{}/{}] {}x 🔊{}%{}",
                s, info.video.title, ch, pos, dur, info.speed, info.volume, eq
            );
        }
        _ => {
            let progress = if info.duration_secs > 0.0 {
                (info.position_secs as f64 / info.duration_secs as f64).min(1.0)
            } else { 0.0 };
            let bar_w: usize = 30;
            let filled = (progress * bar_w as f64) as usize;
            let remaining = bar_w.saturating_sub(filled + 1);
            let bar = format!("{}●{}", "━".repeat(filled), "─".repeat(remaining));

            println!("\n  {} {}", status.bold(), info.video.title.bold());
            println!("  🎵 {}", ch);
            println!("  {} {} {} / {}", bar.green(), pos.cyan(), "/".dimmed(), dur.dimmed());
            println!(
                "  🔊 {}%  ·  {}x  ·  {}  {}  🎛️{}\n",
                info.volume,
                info.speed,
                info.repeat.label(),
                if info.shuffle { "🔀" } else { "" },
                info.eq_preset,
            );
            if let Some(deadline) = info.sleep_deadline {
                println!("  😴 Sleep at {}", deadline.format("%H:%M"));
            }
        }
    }
    Ok(())
}

pub async fn cmd_seek(position: &str) -> Result<()> {
    let remote = RemoteSession::connect()?;

    if position.starts_with('+') || position.starts_with('-') {
        let offset: f64 = position
            .parse()
            .with_context(|| format!("Invalid offset: '{}'. Use +10 or -30.", position))?;
        remote.player.seek(offset).await?;
        let sign = if offset > 0.0 { "+" } else { "" };
        println!("  ⏩ Seeked {}{:.0}s", sign, offset);
    } else {
        let secs = parse_timestamp(position)?;
        remote.player.seek_to(secs).await?;
        println!("  ⏩ Seeked to {}", format_duration(secs as u64));
    }
    Ok(())
}

pub async fn cmd_volume(level: Option<u8>) -> Result<()> {
    let remote = RemoteSession::connect()?;
    match level {
        None => {
            let vol = remote.player.get_volume().await.unwrap_or(0);
            println!("  🔊 Volume: {}%", vol);
        }
        Some(vol) => {
            remote.set_volume(vol).await?;
            println!("  🔊 Volume: {}%", vol.min(100));
        }
    }
    Ok(())
}

pub async fn cmd_speed(value: Option<&str>) -> Result<()> {
    let remote = RemoteSession::connect()?;
    match value {
        None => {
            let speed = remote.player.get_speed().await.unwrap_or(1.0);
            println!("  ⚡ Speed: {}x", speed);
        }
        Some("up") => {
            let current = remote.player.get_speed().await.unwrap_or(1.0);
            let next = next_speed_preset(current, true);
            remote.set_speed(next).await?;
            println!("  ⚡ Speed: {}x → {}x", current, next);
        }
        Some("down") => {
            let current = remote.player.get_speed().await.unwrap_or(1.0);
            let next = next_speed_preset(current, false);
            remote.set_speed(next).await?;
            println!("  ⚡ Speed: {}x → {}x", current, next);
        }
        Some(v) => {
            let speed: f64 = v.parse().context("Speed must be a number (0.25-4.0)")?;
            remote.set_speed(speed).await?;
            println!("  ⚡ Speed: {}x", speed.clamp(0.25, 4.0));
        }
    }
    Ok(())
}

pub async fn cmd_repeat(mode: Option<RepeatMode>) -> Result<()> {
    let remote = RemoteSession::connect()?;
    let next = mode.unwrap_or_else(|| remote.state.repeat.cycle());
    remote.set_repeat(next).await?;
    println!("  🔁 Repeat: {}", next.label());
    Ok(())
}

pub async fn cmd_shuffle() -> Result<()> {
    let remote = RemoteSession::connect()?;
    let enabled = remote.toggle_shuffle().await?;
    println!(
        "  🔀 Shuffle: {}",
        if enabled { "on" } else { "off" }
    );
    Ok(())
}

pub async fn cmd_sleep(duration: &str) -> Result<()> {
    use chrono::Utc;

    if duration == "off" {
        let mut state = StateFile::read()?;
        state.sleep_deadline = None;
        state.write()?;
        println!("  😴 Sleep timer cancelled");
        return Ok(());
    }

    let minutes = parse_duration_str(duration)?;
    let deadline = Utc::now() + chrono::Duration::minutes(minutes as i64);

    let mut state = StateFile::read()?;
    state.sleep_deadline = Some(deadline);
    state.write()?;

    println!(
        "  😴 Sleep timer: {} min (stops at {})",
        minutes,
        deadline.format("%H:%M")
    );
    Ok(())
}

pub async fn cmd_next() -> Result<()> {
    let remote = RemoteSession::connect()?;
    // Seek far ahead to trigger end-of-track; daemon's tick() will detect and advance
    remote.player.seek_to(999999.0).await.ok();
    println!("  ⏭ Skipped to next track");
    Ok(())
}

pub async fn cmd_prev() -> Result<()> {
    let remote = RemoteSession::connect()?;
    // Seek to beginning; if daemon is running it will handle prev logic
    remote.player.seek_to(0.0).await.ok();
    println!("  ⏮ Restarted / previous track");
    Ok(())
}

