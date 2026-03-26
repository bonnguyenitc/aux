use anyhow::Result;
use colored::Colorize;
use std::collections::{HashMap, HashSet};

use crate::library::db::Database;
use crate::library::history::{get_all_history, get_history_since};
use crate::util::format_duration_long;

pub fn cmd_stats(db: &Database, range: &str) -> Result<()> {
    use chrono::Utc;

    let since: Option<String> = match range {
        "today" => Some(Utc::now().format("%Y-%m-%d 00:00:00").to_string()),
        "week" => Some(
            (Utc::now() - chrono::Duration::days(7))
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
        ),
        "month" => Some(
            (Utc::now() - chrono::Duration::days(30))
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
        ),
        _ => None,
    };

    let entries = match since {
        Some(ref s) => get_history_since(db, s)?,
        None => get_all_history(db)?,
    };

    let term_w = crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(80);
    let divider = "─".repeat(term_w.min(60));

    println!("\n  {} Listening Stats ({})", "📊".bold(), range.cyan());
    println!("  {}", divider.dimmed());

    if entries.is_empty() {
        println!("  {} No listening history for this range.\n", "🎵".dimmed());
        return Ok(());
    }

    let total_listened: u64 = entries.iter().map(|e| e.listened_secs.max(0) as u64).sum();
    let total_tracks = entries.len();
    let unique_channels: HashSet<_> = entries
        .iter()
        .filter_map(|e| e.channel.as_deref())
        .collect();

    let mut channel_time: HashMap<&str, u64> = HashMap::new();
    for e in &entries {
        if let Some(ch) = e.channel.as_deref() {
            *channel_time.entry(ch).or_default() += e.listened_secs.max(0) as u64;
        }
    }
    let mut top_channels: Vec<_> = channel_time.into_iter().collect();
    top_channels.sort_by(|a, b| b.1.cmp(&a.1));

    println!("  🎵 {} tracks played", format!("{}", total_tracks).bold());
    println!(
        "  ⏱  {} total listening time",
        format_duration_long(total_listened).green()
    );
    println!(
        "  📺 {} unique channels",
        format!("{}", unique_channels.len()).bold()
    );
    println!("  {}", divider.dimmed());

    if !top_channels.is_empty() {
        println!("  {} Top Channels:", "🏆".bold());
        let max_secs = top_channels.first().map(|(_, s)| *s).unwrap_or(1);
        for (i, (ch, secs)) in top_channels.iter().take(5).enumerate() {
            let bar_len = ((*secs as f64 / max_secs as f64) * 20.0) as usize;
            let bar = "█".repeat(bar_len);
            println!(
                "  {}. {:.<22} {} {}",
                format!("{}", i + 1).dimmed(),
                ch.bold(),
                bar.green(),
                format_duration_long(*secs).dimmed()
            );
        }
    }
    println!();
    Ok(())
}
