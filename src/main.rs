mod ai;
mod commands;
mod cli;
mod config;
mod config_cmd;
mod error;
mod interactive;
mod library;
mod player;
mod tui;
mod util;
mod youtube;

use anyhow::Result;
use clap::Parser;
use colored::*;
use dialoguer::{theme::ColorfulTheme, Select};
use std::io::{self, Write};
use std::time::Instant;

use ai::{ai_chat, fetch_transcript, VideoContext};
use cli::{Cli, Commands, AiAction, ConfigAction, FavAction, PlayerAction, QueueAction, YoutubeAction};
use config::Config;
use interactive::{run_interactive, InteractiveAction};
use library::Database;
use player::{MediaPlayer, MpvPlayer};
use youtube::{YouTubeBackend, YtDlp};

#[tokio::main]
async fn main() -> Result<()> {
    // Handle Ctrl+C gracefully
    ctrlc::set_handler(move || {
        let _ = std::fs::remove_file("/tmp/duet-mpv.sock");
        crossterm::terminal::disable_raw_mode().ok();
        std::process::exit(0);
    })?;

    let cli = Cli::parse();

    // Startup checks
    YtDlp::check_available().await?;
    MpvPlayer::check_available().await?;

    let config = Config::load()?;
    let db = Database::open()?;

    let command = match cli.command {
        Some(cmd) => cmd,
        None => {
            // No subcommand = launch TUI
            run_tui(&config, &db).await?;
            return Ok(());
        }
    };

    match command {
        Commands::Search { query, limit } => {
            cmd_search(&query.join(" "), limit, &config, &db).await?;
        }
        Commands::Play { url, daemon, speed, repeat } => {
            cmd_play(&url, &config, &db).await?;
            // Apply initial speed/repeat to state file after player starts
            if speed.is_some() || repeat.is_some() {
                if let Ok(mut state) = crate::player::state::StateFile::read() {
                    if let Some(s) = speed {
                        state.speed = s.clamp(0.25, 4.0);
                    }
                    if let Some(r) = repeat {
                        use crate::cli::RepeatArg;
                        state.repeat = match r {
                            RepeatArg::Off => crate::player::RepeatMode::Off,
                            RepeatArg::One => crate::player::RepeatMode::One,
                            RepeatArg::All => crate::player::RepeatMode::All,
                        };
                    }
                    state.write().ok();
                }
            }
            if daemon {
                println!("  ℹ️  Tip: detach with Ctrl+Z then `bg` to run in background");
            }
        }
        Commands::Now { format } => {
            let fmt = match format {
                crate::cli::OutputFormat::Pretty => "pretty",
                crate::cli::OutputFormat::Json => "json",
                crate::cli::OutputFormat::Oneline => "oneline",
            };
            crate::commands::playback::cmd_now(fmt).await?;
        }
        Commands::Pause => {
            let remote = crate::player::RemoteSession::connect()
                .map_err(|_| anyhow::anyhow!("No active duet session. Start one with: duet play <url>"))?;
            remote.pause().await?;
            println!("  ⏸ Paused");
        }
        Commands::Resume => {
            let remote = crate::player::RemoteSession::connect()
                .map_err(|_| anyhow::anyhow!("No active duet session. Start one with: duet play <url>"))?;
            remote.resume().await?;
            println!("  ▶ Resumed");
        }
        Commands::Stop => {
            let remote = crate::player::RemoteSession::connect()
                .map_err(|_| anyhow::anyhow!("No active duet session. Start one with: duet play <url>"))?;
            remote.stop().await?;
        }
        Commands::Volume { level } => {
            crate::commands::playback::cmd_volume(level).await?;
        }
        Commands::Seek { position } => {
            crate::commands::playback::cmd_seek(&position).await?;
        }
        Commands::Next => {
            crate::commands::playback::cmd_next().await?;
        }
        Commands::Prev => {
            crate::commands::playback::cmd_prev().await?;
        }
        Commands::Speed { value } => {
            crate::commands::playback::cmd_speed(value.as_deref()).await?;
        }
        Commands::Repeat { mode } => {
            use crate::cli::RepeatArg;
            let repeat_mode = mode.map(|m| match m {
                RepeatArg::Off => crate::player::RepeatMode::Off,
                RepeatArg::One => crate::player::RepeatMode::One,
                RepeatArg::All => crate::player::RepeatMode::All,
            });
            crate::commands::playback::cmd_repeat(repeat_mode).await?;
        }
        Commands::Shuffle => {
            crate::commands::playback::cmd_shuffle().await?;
        }
        Commands::Sleep { duration } => {
            crate::commands::playback::cmd_sleep(&duration).await?;
        }
        Commands::Stats { range } => {
            crate::commands::stats::cmd_stats(&db, &range)?;
        }
        Commands::Logs { lines, follow: _ } => {
            let log_path = directories::ProjectDirs::from("", "", "duet")
                .map(|d| d.data_dir().join("daemon.log"))
                .unwrap_or_else(|| std::path::PathBuf::from("/tmp/duet-daemon.log"));
            if log_path.exists() {
                let content = std::fs::read_to_string(&log_path)?;
                let all_lines: Vec<&str> = content.lines().collect();
                let start = all_lines.len().saturating_sub(lines);
                for line in &all_lines[start..] {
                    println!("{}", line);
                }
            } else {
                println!("  No daemon log found at {}", log_path.display());
            }
        }
        Commands::Chat { message: _, model: _, profile: _ } => {
            println!(
                "  {}",
                "Chat requires a video playing. Use: duet search → play → [c] to chat".dimmed()
            );
        }
        Commands::Suggest { model: _, profile: _ } => {
            println!(
                "  {}",
                "Suggest requires a video playing. Use: duet search → play → [c] to chat"
                    .dimmed()
            );
        }
        Commands::History { limit, today } => {
            cmd_history(&db, limit, today)?;
        }
        Commands::Favorites { action } => {
            cmd_favorites(&db, action).await?;
        }
        Commands::Queue { action } => {
            cmd_queue(&db, action, &config).await?;
        }
        Commands::Config { action } => {
            let mut cfg = Config::load()?;
            cmd_config(action, &mut cfg).await?;
        }
    }

    Ok(())
}



/// Returns Some(new_query) if user wants to search again, None if user quit
async fn play_video(
    video: &youtube::VideoInfo,
    config: &Config,
    db: &Database,
) -> Result<Option<String>> {
    let yt = YtDlp::new();
    let started_at = Instant::now();

    println!(
        "  {} {}",
        "▶ Playing:".green().bold(),
        video.title.bold()
    );
    println!(
        "  {} {}",
        "🎵 Channel:".dimmed(),
        video.channel.as_deref().unwrap_or("Unknown")
    );

    // Check if favorite
    let is_fav = library::favorites::is_favorite(db, &video.id).unwrap_or(false);
    if is_fav {
        println!("  {} favorited", "❤️");
    }

    // Fetch transcript (3-tier: manual subs → auto subs → description)
    println!("  {} fetching transcript...", "📝".dimmed());
    let transcript = fetch_transcript(&video.url).await.unwrap_or(None);
    match &transcript {
        Some(t) if t.language == "description" => println!(
            "  {} using video description as context ({} chars)",
            "📄".yellow(),
            t.segments.first().map_or(0, |s| s.text.len()),
        ),
        Some(t) => println!(
            "  {} transcript loaded ({} segments, lang: {})",
            "✅".green(),
            t.segments.len(),
            t.language
        ),
        None => println!("  {} no transcript or description available", "⚠️".yellow()),
    }

    // Create AI context
    let mut ai_context = VideoContext::new(video.clone(), transcript);

    println!();

    // Get stream URL and play
    let stream = yt.get_stream_url(&video.url).await?;

    let mut player = MpvPlayer::new();
    player.play(&stream.audio_url, &video.title).await?;

    // Print player UI header (reused after returning from chat)
    print_player_ui(video);

    // Enter interactive mode
    let result = loop {
        match run_interactive(&mut player, video, db).await? {
            InteractiveAction::Quit => {
                player.stop().await?;
                println!("\n  {} 👋", "Stopped.".dimmed());
                break None;
            }
            InteractiveAction::NewSearch => {
                player.stop().await?;
                crossterm::terminal::disable_raw_mode().ok();
                let input: String = dialoguer::Input::new()
                    .with_prompt("🔍 Search")
                    .interact_text()?;
                break Some(input);
            }
            InteractiveAction::Chat => {
                crossterm::terminal::disable_raw_mode().ok();

                if let Ok(pos) = player.get_position().await {
                    ai_context.current_position = pos;
                }

                run_chat_mode(&mut ai_context, config).await?;

                // Restore player UI so user sees context after chat
                print_player_ui(video);
            }
        }
    };

    // Record to history
    let listened_secs = started_at.elapsed().as_secs();
    library::history::add_to_history(db, video, listened_secs)?;

    Ok(result)
}

// ─── Player UI header ────────────────────────────────────────

/// Print (or re-print) the now-playing header and keybind legend.
/// Called at initial playback start and again after returning from chat mode.
fn print_player_ui(video: &youtube::VideoInfo) {
    use colored::Colorize;
    println!();
    println!(
        "  {} {}",
        "▶ Now playing:".green().bold(),
        video.title.bold()
    );
    println!(
        "  {} {}",
        "🎵 Channel:".dimmed(),
        video.channel.as_deref().unwrap_or("Unknown").dimmed()
    );
    println!(
        "  {} pause  {} seek±10s  {} seek±30s  {} vol  {} speed  {} repeat  {} shuffle  {} fav  {} queue  {} search  {} chat  {} quit",
        "[spc]".cyan(), "[←→]".cyan(), "[⇧←→]".cyan(), "[↑↓]".cyan(),
        "[+/-]".cyan(), "[r]".cyan(), "[x]".cyan(), "[f]".cyan(),
        "[a]".cyan(), "[s]".cyan(), "[c]".cyan(), "[q]".cyan(),
    );
    println!();
}

// ─── Chat ────────────────────────────────────────────────────

async fn run_chat_mode(context: &mut VideoContext, config: &Config) -> Result<()> {
    println!(
        "\n  {} {} {}\n",
        "💬 Chat mode".bold().cyan(),
        "(playing:".dimmed(),
        format!("{})", context.video.title).dimmed()
    );

    if config.ai.is_none() {
        println!(
            "  {}",
            "⚠️  AI not configured. Run: duet config ai --setup".yellow()
        );
        println!();
        return Ok(());
    }

    // Resolve AI config (default profile, no overrides)
    let resolved = config
        .ai
        .as_ref()
        .unwrap()
        .resolve(None)?;

    println!("  {} Esc to exit chat  /quit or /q to quit\n", "💡".dimmed());

    loop {
        // Read one line from the user with ESC support via crossterm raw mode.
        // Returns None if the user pressed ESC, Some(String) otherwise.
        let input = read_chat_input()?;

        let input = match input {
            None => break,                    // ESC → quit chat
            Some(s) => s,
        };
        let input = input.trim().to_string();

        if input.is_empty() {
            continue;
        }
        if input == "/quit" || input == "/q" || input == "exit" {
            break;
        }

        let spinner = indicatif::ProgressBar::new_spinner();
        spinner.set_style(
            indicatif::ProgressStyle::with_template("  🤖:{spinner:.cyan} {msg}")
                .unwrap()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        spinner.set_message("Thinking...");
        spinner.enable_steady_tick(std::time::Duration::from_millis(80));

        match ai_chat(context, &input, &resolved).await {
            Ok(response) => {
                spinner.finish_and_clear();
                println!("  {} {}\n", "🤖:".bold().cyan(), response);
            }
            Err(e) => {
                spinner.finish_and_clear();
                println!("  {} {}\n", "🤖:".bold().cyan(), format!("Error: {}", e).red());
            }
        }
    }

    println!("  {} Chat ended.\n", "👋".dimmed());

    Ok(())
}

/// Read a line of input from the terminal using crossterm raw mode.
/// Returns `None` if the user pressed `Esc` or `Ctrl+C`.
/// Returns `Some(String)` when the user hits `Enter`.
fn read_chat_input() -> anyhow::Result<Option<String>> {
    use crossterm::{
        event::{self, Event, KeyCode, KeyModifiers},
        terminal::{disable_raw_mode, enable_raw_mode},
    };
    // std::io::Write already in scope from top-level use

    print!("  {} ", "You:".bold().green());
    io::stdout().flush()?;

    enable_raw_mode()?;

    let mut buf = String::new();
    let result = loop {
        let ev = event::read()?;
        match ev {
            // Enter → submit
            Event::Key(k) if k.code == KeyCode::Enter => {
                break Some(buf);
            }
            // ESC or Ctrl+C → quit
            Event::Key(k)
                if k.code == KeyCode::Esc
                    || (k.code == KeyCode::Char('c')
                        && k.modifiers.contains(KeyModifiers::CONTROL)) =>
            {
                break None;
            }
            // Backspace
            Event::Key(k) if k.code == KeyCode::Backspace => {
                if buf.pop().is_some() {
                    // Move cursor back, erase char, move cursor back again
                    print!("\x08 \x08");
                    io::stdout().flush()?;
                }
            }
            // Printable character
            Event::Key(k) => {
                if let KeyCode::Char(c) = k.code {
                    if k.modifiers.is_empty() || k.modifiers == KeyModifiers::SHIFT {
                        buf.push(c);
                        print!("{}", c);
                        io::stdout().flush()?;
                    }
                }
            }
            _ => {}
        }
    };

    disable_raw_mode()?;
    println!(); // newline after the user's input
    Ok(result)
}


async fn cmd_search(query: &str, limit: usize, config: &Config, db: &Database) -> Result<()> {
    use colored::Colorize;
    use std::collections::HashSet;

    let mut current_query = query.to_string();
    let page_size = if limit > 0 {
        limit
    } else {
        config.player.search_results
    };

    loop {
        println!("\n  {} {}\n", "🔍 Searching:".bold(), current_query);

        let yt = YtDlp::new();
        // Fetch a larger batch for local pagination (up to 5 pages)
        let max_fetch = page_size * 5;
        let all_results = yt.search(&current_query, max_fetch).await?;
        // Persist to search history
        library::search_history::add_search(db, &current_query).ok();

        // Pre-fetch which videos are already in favorites / queue
        let fav_ids: HashSet<String> = library::favorites::get_favorites(db)
            .unwrap_or_default()
            .into_iter()
            .map(|e| e.video_id)
            .collect();
        let queue_ids: HashSet<String> = library::queue::get_queue(db)
            .unwrap_or_default()
            .into_iter()
            .map(|e| e.video_id)
            .collect();

        let total = all_results.len();
        let total_pages = (total + page_size - 1) / page_size; // ceil division
        let mut current_page: usize = 0;

        let action = loop {
            let start = current_page * page_size;
            let end = (start + page_size).min(total);
            let page_results = &all_results[start..end];

            let items: Vec<String> = page_results
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let global_idx = start + i + 1;
                    let duration = v
                        .duration
                        .map(|d| youtube::types::format_duration(d as u64))
                        .unwrap_or_else(|| "LIVE".to_string());
                    let channel = v.channel.as_deref().unwrap_or("Unknown");
                    let fav   = if fav_ids.contains(&v.id)   { "❤️ " } else { "" };
                    let queue = if queue_ids.contains(&v.id)  { "📋" } else { "" };
                    format!("{}. {}{}{} — {} [{}]", global_idx, fav, queue, v.title, channel, duration)
                })
                .collect();

            let page_info = PageInfo {
                current: current_page,
                total: total_pages,
            };

            match select_video(&items, &current_query, &page_info)? {
                SelectAction::Selected(idx) => {
                    break SelectAction::Selected(start + idx);
                }
                SelectAction::Cancelled => {
                    break SelectAction::Cancelled;
                }
                SelectAction::NextPage => {
                    if current_page + 1 < total_pages {
                        current_page += 1;
                    }
                }
                SelectAction::PrevPage => {
                    if current_page > 0 {
                        current_page -= 1;
                    }
                }
            }
        };

        match action {
            SelectAction::Selected(idx) => {
                let video = &all_results[idx];
                match play_video(video, config, db).await? {
                    Some(new_query) => {
                        current_query = new_query;
                        continue;
                    }
                    None => break,
                }
            }
            SelectAction::Cancelled => {
                println!("  {}", "Cancelled.".dimmed());
                break;
            }
            _ => unreachable!(),
        }
    }

    Ok(())
}

async fn cmd_play(url: &str, config: &Config, db: &Database) -> Result<()> {
    use colored::Colorize;

    let yt = YtDlp::new();

    println!("\n  {} {}\n", "🔍 Fetching:".bold(), url);

    let video = if youtube::is_youtube_url(url) {
        // Direct URL or video ID — fetch metadata without going through ytsearch:
        yt.fetch_info(url).await?
    } else {
        // Keyword query — use search
        let results = yt.search(url, 1).await?;
        results.into_iter().next().ok_or_else(|| anyhow::anyhow!("No results for: {}", url))?
    };

    play_video(&video, config, db).await?;

    Ok(())
}

// ─── Video Selector ───────────────────────────────────────────
//
// Custom full-screen selector using crossterm raw mode.
// Supports pagination via [←/→] keys.

/// Result of the video selector interaction
enum SelectAction {
    Selected(usize),
    Cancelled,
    NextPage,
    PrevPage,
}

/// Pagination context passed to the selector
struct PageInfo {
    current: usize,
    total: usize,
}

fn select_video(items: &[String], query: &str, page: &PageInfo) -> Result<SelectAction> {
    use crossterm::{
        cursor::{Hide, MoveTo, Show},
        event::{self, Event, KeyCode, KeyModifiers},
        execute, queue,
        style::{
            Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor,
        },
        terminal::{
            disable_raw_mode, enable_raw_mode, size, Clear, ClearType,
            EnterAlternateScreen, LeaveAlternateScreen,
        },
    };

    if items.is_empty() {
        return Ok(SelectAction::Cancelled);
    }

    let mut selected: usize = 0;
    let n = items.len();
    let has_prev = page.current > 0;
    let has_next = page.current + 1 < page.total;

    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen, Hide)?;

    let result = loop {
        let (cols, rows) = size().unwrap_or((120, 40));
        let cols = cols as usize;
        let rows = rows as usize;

        // Header = 3 lines, footer = 3 lines
        let visible = rows.saturating_sub(6).max(1).min(n);

        // Keep selected inside scroll window
        let start = if selected >= visible {
            selected - visible + 1
        } else {
            0
        };

        // ── Full clear + redraw from top-left ──────────────────────────────
        execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0))?;

        let divider = "─".repeat(cols.min(80));

        // Header (3 lines)
        queue!(
            io::stdout(),
            Print(format!(
                "\r\n  🔍 Results for: {}  (page {}/{})\r\n  {}\r\n",
                query,
                page.current + 1,
                page.total,
                divider,
            )),
        )?;

        // Items
        for abs_i in start..(start + visible).min(n) {
            let item = &items[abs_i];
            let display: String = item.chars().take(cols.saturating_sub(6)).collect();

            if abs_i == selected {
                queue!(
                    io::stdout(),
                    SetForegroundColor(Color::Cyan),
                    SetAttribute(Attribute::Bold),
                    Print(format!("  › {}\r\n", display)),
                    ResetColor,
                    SetAttribute(Attribute::Reset),
                )?;
            } else {
                queue!(
                    io::stdout(),
                    Print(format!("    {}\r\n", display)),
                )?;
            }
        }

        // Footer — build navigation hints dynamically
        let mut nav_parts = vec![
            format!("{} navigate", "[↑↓]"),
            format!("{} select", "[Enter]"),
        ];
        if has_prev {
            nav_parts.push(format!("{} prev page", "[←]"));
        }
        if has_next {
            nav_parts.push(format!("{} next page", "[→]"));
        }
        nav_parts.push(format!("{} cancel", "[Esc/q]"));

        let counter = if n > visible {
            format!("  ({}/{})", selected + 1, n)
        } else {
            String::new()
        };
        queue!(
            io::stdout(),
            Print(format!("\r\n  {}\r\n", divider)),
            Print(format!(
                "  {}{}\r\n",
                nav_parts.join("   "),
                counter,
            )),
        )?;

        io::stdout().flush()?;

        // ── Handle key ─────────────────────────────────────────────────────
        if let Event::Key(key) = event::read()? {
            match (key.code, key.modifiers) {
                (KeyCode::Up, _) => {
                    if selected > 0 {
                        selected -= 1;
                    }
                }
                (KeyCode::Down, _) => {
                    if selected < n - 1 {
                        selected += 1;
                    }
                }
                (KeyCode::Enter, _) => break SelectAction::Selected(selected),
                // Page navigation
                (KeyCode::Right, _) if has_next => break SelectAction::NextPage,
                (KeyCode::Left, _) if has_prev => break SelectAction::PrevPage,
                (KeyCode::Esc, _)
                | (KeyCode::Char('q'), _)
                | (KeyCode::Char('c'), KeyModifiers::CONTROL) => break SelectAction::Cancelled,
                _ => {}
            }
        }
    };

    execute!(io::stdout(), Show, LeaveAlternateScreen)?;
    disable_raw_mode()?;

    Ok(result)
}

// ─── History ─────────────────────────────────────────────────

fn cmd_history(db: &Database, limit: usize, today: bool) -> Result<()> {
    let entries = if today {
        println!("\n  {} {}\n", "📜".bold(), "Today's History".bold());
        library::history::get_today_history(db)?
    } else {
        println!("\n  {} {}\n", "📜".bold(), "Play History".bold());
        library::history::get_history(db, limit)?
    };

    if entries.is_empty() {
        println!("  {}", "No history yet. Play some music!".dimmed());
        return Ok(());
    }

    for (i, entry) in entries.iter().enumerate() {
        let channel = entry.channel.as_deref().unwrap_or("Unknown");
        let duration = entry
            .duration_secs
            .map(|d| youtube::types::format_duration(d as u64))
            .unwrap_or_else(|| "?".to_string());
        let listened = youtube::types::format_duration(entry.listened_secs as u64);

        println!(
            "  {}. {} — {} [{}] (listened: {}) — {}",
            i + 1,
            entry.title.bold(),
            channel.dimmed(),
            duration.dimmed(),
            listened.green(),
            entry.played_at.dimmed()
        );
    }
    println!();

    Ok(())
}

// ─── Favorites ───────────────────────────────────────────────

async fn cmd_favorites(db: &Database, action: Option<FavAction>) -> Result<()> {
    match action {
        None | Some(FavAction::List) => {
            let entries = library::favorites::get_favorites(db)?;

            println!("\n  {} {}\n", "❤️".bold(), "Favorites".bold());

            if entries.is_empty() {
                println!(
                    "  {}",
                    "No favorites yet. Press [f] while playing to add one!".dimmed()
                );
                return Ok(());
            }

            let items: Vec<String> = entries
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let duration = v
                        .duration_secs
                        .map(|d| youtube::types::format_duration(d as u64))
                        .unwrap_or_else(|| "?".to_string());
                    let channel = v.channel.as_deref().unwrap_or("Unknown");
                    format!("{}. {} — {} [{}]", i + 1, v.title, channel, duration)
                })
                .collect();

            let selection = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("Play a favorite? (Esc to cancel)")
                .items(&items)
                .default(0)
                .interact_opt()?;

            if let Some(idx) = selection {
                let entry = &entries[idx];
                cmd_play(&entry.url, &Config::load()?, db).await?;
            }
        }
        Some(FavAction::Add { url }) => {
            let yt = YtDlp::new();
            let results = yt.search(&url, 1).await?;
            if let Some(video) = results.first() {
                let added = library::favorites::add_favorite(db, video)?;
                if added {
                    println!("  {} {} added to favorites!", "❤️", video.title.bold());
                } else {
                    println!("  {} already in favorites", "ℹ️".dimmed());
                }
            }
        }
        Some(FavAction::Remove { video_id }) => {
            let removed = library::favorites::remove_favorite(db, &video_id)?;
            if removed {
                println!("  {} removed from favorites", "💔");
            } else {
                println!("  {} not found in favorites", "ℹ️".dimmed());
            }
        }
    }

    Ok(())
}

// ─── Queue ───────────────────────────────────────────────────

async fn cmd_queue(db: &Database, action: Option<QueueAction>, config: &Config) -> Result<()> {
    match action {
        None | Some(QueueAction::List) => {
            let entries = library::queue::get_queue(db)?;

            println!("\n  {} {}\n", "📋".bold(), "Play Queue".bold());

            if entries.is_empty() {
                println!(
                    "  {}",
                    "Queue is empty. Press [a] while playing to add videos!".dimmed()
                );
                return Ok(());
            }

            for (i, entry) in entries.iter().enumerate() {
                let channel = entry.channel.as_deref().unwrap_or("Unknown");
                let duration = entry
                    .duration_secs
                    .map(|d| youtube::types::format_duration(d as u64))
                    .unwrap_or_else(|| "?".to_string());

                let prefix = if i == 0 {
                    "▶".green().to_string()
                } else {
                    format!("{}", i + 1)
                };

                println!(
                    "  {}. {} — {} [{}]",
                    prefix,
                    entry.title.bold(),
                    channel.dimmed(),
                    duration.dimmed()
                );
            }
            println!();
        }
        Some(QueueAction::Add { url }) => {
            let yt = YtDlp::new();
            let results = yt.search(&url, 1).await?;
            if let Some(video) = results.first() {
                if library::queue::add_to_queue(db, video)? {
                    let len = library::queue::queue_length(db)?;
                    println!("  {} {} added to queue (#{})", "📋", video.title.bold(), len);
                } else {
                    println!("  {} {} is already in queue — skipped", "⚠️", video.title.dimmed());
                }
            }
        }
        Some(QueueAction::Next) => {
            if let Some(entry) = library::queue::pop_next(db)? {
                println!("  {} Playing next: {}", "⏭️", entry.title.bold());
                cmd_play(&entry.url, config, db).await?;
            } else {
                println!("  {}", "Queue is empty".dimmed());
            }
        }
        Some(QueueAction::Clear) => {
            let count = library::queue::clear_queue(db)?;
            println!("  {} Cleared {} items from queue", "🗑️", count);
        }
    }

    Ok(())
}

// ─── TUI Mode ────────────────────────────────────────────────

async fn run_tui(config: &Config, db: &Database) -> Result<()> {
    use crossterm::{
        event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    };
    use ratatui::backend::CrosstermBackend;
    use ratatui::Terminal;
    use std::time::Duration;

    // Setup terminal
    use tui::app::{App, NowPlaying, Panel};
    use tui::ui;

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    // Pre-load search history for ↑/↓ recall
    app.search_history = library::search_history::get_searches(db, 100).unwrap_or_default();
    let yt = YtDlp::new();
    let mut player: Option<MpvPlayer> = None;
    let mut ai_context: Option<VideoContext> = None;

    loop {
        // Draw
        terminal.draw(|frame| {
            ui::draw(frame, &app);
        })?;

        // Update playback info from mpv
        if let Some(ref p) = player {
            let pos = p.get_position().await.map(|d| d.as_secs()).unwrap_or(0);
            let dur = p.get_duration().await.map(|d| d.as_secs()).unwrap_or(0);
            let vol = p.get_volume().await.unwrap_or(80);
            let paused = app.now_playing.as_ref().map(|np| np.paused).unwrap_or(false);
            app.update_playback(pos, dur, paused, vol);

            // Keep AI context position in sync
            if let Some(ref mut ctx) = ai_context {
                ctx.current_position = Duration::from_secs(pos);
            }

            // Sync speed/repeat/shuffle/sleep from state file
            if let Ok(state) = crate::player::state::StateFile::read() {
                app.update_player_meta(state.speed, state.repeat, state.shuffle, state.sleep_deadline);
            }
        }

        // Handle events
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(KeyEvent { code, modifiers, .. }) = event::read()? {

                // Universal: Tab switches panels
                if code == KeyCode::Tab {
                    let next = match app.panel {
                        Panel::Search   => Panel::Results,
                        Panel::Results  => Panel::Queue,
                        Panel::Queue    => Panel::History,
                        Panel::History  => Panel::Chat,
                        Panel::Chat     => Panel::Help,
                        Panel::Help     => Panel::Search,
                    };
                    // Preload data for panels that need it
                    match next {
                        Panel::Queue => {
                            app.queue_items = library::queue::get_queue(db).unwrap_or_default();
                        }
                        Panel::History => {
                            app.history_items = library::history::get_history(db, 50).unwrap_or_default();
                        }
                        _ => {}
                    }
                    app.set_panel(next);
                    continue;
                }

                let mut handled = false;

                match app.panel {
                    Panel::Search => match code {
                        KeyCode::Esc => {
                            app.should_quit = true;
                        }
                        KeyCode::Char('q') => {
                            app.should_quit = true;
                        }
                        KeyCode::Char('?') => {
                            app.set_panel(Panel::Help);
                        }
                        // ── History navigation ──────────────────────────────
                        KeyCode::Up => {
                            app.search_history_up();
                            handled = true; // don't let global volume handle ↑
                        }
                        KeyCode::Down => {
                            app.search_history_down();
                            handled = true;
                        }
                        // ── Execute search ──────────────────────────────────
                        KeyCode::Enter => {
                            if !app.search_input.is_empty() {
                                let query = app.search_input.clone();
                                app.cancel_search_history_nav();
                                app.set_status("Searching...");
                                terminal.draw(|f| ui::draw(f, &app))?;

                                // Fetch larger batch for local pagination
                                let fetch_count = app.search_page_size * 5;
                                match yt.search(&query, fetch_count).await {
                                    Ok(results) => {
                                        // Persist to search history
                                        library::search_history::add_search(db, &query).ok();
                                        app.search_history =
                                            library::search_history::get_searches(db, 100)
                                                .unwrap_or_default();

                                        let total = results.len();
                                        app.set_search_results(results);
                                        app.set_panel(Panel::Results);
                                        app.set_status(format!(
                                            "Found {} results (page 1/{})",
                                            total,
                                            app.search_total_pages()
                                        ));
                                    }
                                    Err(e) => {
                                        app.set_status(format!("Search failed: {}", e));
                                    }
                                }
                            }
                        }
                        KeyCode::Backspace => {
                            app.cancel_search_history_nav();
                            app.search_input.pop();
                        }
                        KeyCode::Char(c) => {
                            app.cancel_search_history_nav();
                            app.search_input.push(c);
                            // All printable chars consumed by search — don't leak
                            // to global player controls (e.g. space = pause)
                            handled = true;
                        }
                        _ => {}
                    },

                    Panel::Results => match code {
                        KeyCode::Esc | KeyCode::Char('q') => {
                            app.set_panel(Panel::Search);
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            app.select_prev();
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            app.select_next();
                        }
                        KeyCode::Enter => {
                            if let Some(video) = app.search_results.get(app.selected_index).cloned() {
                                app.set_status(format!("Loading: {}...", video.title));
                                terminal.draw(|f| ui::draw(f, &app))?;

                                match yt.get_stream_url(&video.url).await {
                                    Ok(stream) => {
                                        if let Some(mut old) = player.take() {
                                            old.stop().await.ok();
                                        }
                                        let mut p = MpvPlayer::new();
                                        if p.play(&stream.audio_url, &video.title).await.is_ok() {
                                            let is_fav =
                                                library::favorites::is_favorite(db, &video.id)
                                                    .unwrap_or(false);
                                            let in_queue =
                                                library::queue::is_in_queue(db, &video.id)
                                                    .unwrap_or(false);

                                            // Write state file
                                            let state = crate::player::state::StateFile::new(
                                                video.clone(),
                                                false,
                                            );
                                            state.write().ok();

                                            app.now_playing = Some(NowPlaying {
                                                video: video.clone(),
                                                position_secs: 0,
                                                duration_secs: video.duration.unwrap_or(0.0) as u64,
                                                paused: false,
                                                volume: 80,
                                                speed: 1.0,
                                                repeat: crate::player::RepeatMode::Off,
                                                shuffle: false,
                                                is_fav,
                                                in_queue,
                                                sleep_deadline: None,
                                            });

                                            library::history::add_to_history(db, &video, 0).ok();
                                            player = Some(p);
                                            // Fetch transcript for AI chat context
                                            app.set_status(format!("Playing: {} — loading transcript…", video.title));
                                            terminal.draw(|f| ui::draw(f, &app))?;
                                            let transcript = fetch_transcript(&video.url).await.unwrap_or(None);
                                            ai_context = Some(VideoContext::new(video.clone(), transcript));
                                            app.chat_messages.clear();
                                            app.chat_scroll = 0;
                                            app.set_status(format!("Playing: {}", video.title));
                                        }
                                    }
                                    Err(e) => {
                                        app.set_status(format!("Failed to play: {}", e));
                                    }
                                }
                            }
                        }
                        KeyCode::Char('a') => {
                            if let Some(video) = app.search_results.get(app.selected_index) {
                                match library::queue::add_to_queue(db, video) {
                                    Ok(true) => {
                                        let len = library::queue::queue_length(db).unwrap_or(0);
                                        app.set_status(format!("Added to queue (#{}) ✓", len));
                                    }
                                    Ok(false) => {
                                        app.set_status("⚠ Already in queue — skipped".to_string());
                                    }
                                    Err(e) => app.set_status(format!("Queue error: {}", e)),
                                }
                            }
                        }
                        KeyCode::Char('/') => {
                            app.set_panel(Panel::Search);
                            app.search_input.clear();
                        }
                        // ── Page navigation ──────────────────────────────
                        KeyCode::Right => {
                            if app.search_page + 1 < app.search_total_pages() {
                                app.search_next_page();
                                app.set_status(format!(
                                    "Page {}/{}",
                                    app.search_page + 1,
                                    app.search_total_pages()
                                ));
                            }
                        }
                        KeyCode::Left => {
                            if app.search_page > 0 {
                                app.search_prev_page();
                                app.set_status(format!(
                                    "Page {}/{}",
                                    app.search_page + 1,
                                    app.search_total_pages()
                                ));
                            }
                        }
                        _ => {}
                    },

                    Panel::Queue => match code {
                        KeyCode::Esc | KeyCode::Char('q') => {
                            app.set_panel(Panel::Search);
                        }
                        KeyCode::Up | KeyCode::Char('k') => { app.select_prev(); }
                        KeyCode::Down | KeyCode::Char('j') => { app.select_next(); }
                        KeyCode::Enter => {
                            // Play selected queue item
                            if let Some(entry) = app.queue_items.get(app.selected_index).cloned() {
                                app.set_status(format!("Loading: {}...", entry.title));
                                terminal.draw(|f| ui::draw(f, &app))?;

                                let fake_video = crate::youtube::VideoInfo {
                                    id: entry.video_id.clone(),
                                    title: entry.title.clone(),
                                    channel: entry.channel.clone(),
                                    duration: entry.duration_secs.map(|s| s as f64),
                                    view_count: None,
                                    thumbnail: None,
                                    url: entry.url.clone(),
                                    description: None,
                                };

                                match yt.get_stream_url(&entry.url).await {
                                    Ok(stream) => {
                                        if let Some(mut old) = player.take() { old.stop().await.ok(); }
                                        let mut p = MpvPlayer::new();
                                        if p.play(&stream.audio_url, &entry.title).await.is_ok() {
                                            let state = crate::player::state::StateFile::new(fake_video.clone(), false);
                                            state.write().ok();
                                            app.now_playing = Some(NowPlaying {
                                                video: fake_video,
                                                position_secs: 0,
                                                duration_secs: entry.duration_secs.unwrap_or(0) as u64,
                                                paused: false,
                                                volume: 80,
                                                speed: 1.0,
                                                repeat: crate::player::RepeatMode::Off,
                                                shuffle: false,
                                                is_fav: false,
                                                in_queue: true, // playing from queue
                                                sleep_deadline: None,
                                            });
                                            player = Some(p);
                                            // Fetch transcript for AI chat context
                                            app.set_status(format!("Playing: {} — loading transcript…", entry.title));
                                            terminal.draw(|f| ui::draw(f, &app))?;
                                            let transcript = fetch_transcript(&entry.url).await.unwrap_or(None);
                                            let vi = crate::youtube::VideoInfo {
                                                id: entry.video_id.clone(),
                                                title: entry.title.clone(),
                                                url: entry.url.clone(),
                                                channel: entry.channel.clone(),
                                                duration: entry.duration_secs.map(|d| d as f64),
                                                view_count: None,
                                                thumbnail: None,
                                                description: None,
                                            };
                                            ai_context = Some(VideoContext::new(vi, transcript));
                                            app.chat_messages.clear();
                                            app.chat_scroll = 0;
                                            app.set_status(format!("Playing: {}", entry.title));
                                        }
                                    }
                                    Err(e) => { app.set_status(format!("Failed: {}", e)); }
                                }
                            }
                        }
                        KeyCode::Char('d') => {
                            // Copy id out first to release the immutable borrow on app.queue_items
                            let entry_id = app.queue_items
                                .get(app.selected_index)
                                .map(|e| e.id);
                            if let Some(id) = entry_id {
                                match library::queue::remove_from_queue(db, id) {
                                    Ok(true) => {
                                        // Reload queue and clamp index
                                        app.queue_items = library::queue::get_queue(db)
                                            .unwrap_or_default();
                                        if app.selected_index >= app.queue_items.len()
                                            && !app.queue_items.is_empty()
                                        {
                                            app.selected_index = app.queue_items.len() - 1;
                                        } else if app.queue_items.is_empty() {
                                            app.selected_index = 0;
                                        }
                                        app.set_status("Removed from queue");
                                    }
                                    Ok(false) => {
                                        app.set_status("Item not found in queue");
                                    }
                                    Err(e) => {
                                        app.set_status(format!("Remove failed: {}", e));
                                    }
                                }
                            }
                        }
                        _ => {}
                    },

                    Panel::History => match code {
                        KeyCode::Esc | KeyCode::Char('q') => {
                            app.set_panel(Panel::Search);
                        }
                        KeyCode::Up | KeyCode::Char('k') => { app.select_prev(); }
                        KeyCode::Down | KeyCode::Char('j') => { app.select_next(); }
                        KeyCode::Enter => {
                            if let Some(entry) = app.history_items.get(app.selected_index).cloned() {
                                app.set_status(format!("Loading: {}...", entry.title));
                                terminal.draw(|f| ui::draw(f, &app))?;
                                match yt.get_stream_url(&entry.url).await {
                                    Ok(stream) => {
                                        if let Some(mut old) = player.take() { old.stop().await.ok(); }
                                        let mut p = MpvPlayer::new();
                                        if p.play(&stream.audio_url, &entry.title).await.is_ok() {
                                            let fake_video = crate::youtube::VideoInfo {
                                                id: entry.video_id.clone(),
                                                title: entry.title.clone(),
                                                channel: entry.channel.clone(),
                                                duration: entry.duration_secs.map(|s| s as f64),
                                                view_count: None,
                                                thumbnail: None,
                                                url: entry.url.clone(),
                                                description: None,
                                            };
                                            let state = crate::player::state::StateFile::new(fake_video.clone(), false);
                                            state.write().ok();
                                            let is_fav =
                                                library::favorites::is_favorite(db, &entry.video_id)
                                                    .unwrap_or(false);
                                            let in_queue =
                                                library::queue::is_in_queue(db, &entry.video_id)
                                                    .unwrap_or(false);
                                            app.now_playing = Some(NowPlaying {
                                                video: fake_video,
                                                position_secs: 0,
                                                duration_secs: entry.duration_secs.unwrap_or(0) as u64,
                                                paused: false,
                                                volume: 80,
                                                speed: 1.0,
                                                repeat: crate::player::RepeatMode::Off,
                                                shuffle: false,
                                                is_fav,
                                                in_queue,
                                                sleep_deadline: None,
                                            });
                                            player = Some(p);
                                            // Fetch transcript for AI chat context
                                            app.set_status(format!("Playing: {} — loading transcript…", entry.title));
                                            terminal.draw(|f| ui::draw(f, &app))?;
                                            let transcript = fetch_transcript(&entry.url).await.unwrap_or(None);
                                            let vi = crate::youtube::VideoInfo {
                                                id: entry.video_id.clone(),
                                                title: entry.title.clone(),
                                                url: entry.url.clone(),
                                                channel: entry.channel.clone(),
                                                duration: entry.duration_secs.map(|d| d as f64),
                                                view_count: None,
                                                thumbnail: None,
                                                description: None,
                                            };
                                            ai_context = Some(VideoContext::new(vi, transcript));
                                            app.chat_messages.clear();
                                            app.chat_scroll = 0;
                                            app.set_status(format!("Playing: {}", entry.title));
                                        }
                                    }
                                    Err(e) => { app.set_status(format!("Failed: {}", e)); }
                                }
                            }
                        }
                        _ => {}
                    },

                    Panel::Chat => match code {
                        KeyCode::Esc | KeyCode::Char('q') => {
                            app.set_panel(Panel::Search);
                        }
                        KeyCode::Enter => {
                            let input = app.chat_input.trim().to_string();
                            if !input.is_empty() {
                                app.chat_input.clear();
                                app.push_chat_message("user", &input);

                                if let Some(ref mut ctx) = ai_context {
                                    if let Some(ref ai_cfg) = config.ai {
                                        match ai_cfg.resolve(None) {
                                            Ok(resolved) => {
                                                app.chat_loading = true;
                                                app.set_status("AI is thinking...");
                                                terminal.draw(|f| ui::draw(f, &app))?;

                                                match ai_chat(ctx, &input, &resolved).await {
                                                    Ok(response) => {
                                                        app.push_chat_message("assistant", &response);
                                                        app.set_status("Reply received");
                                                    }
                                                    Err(e) => {
                                                        app.push_chat_message("assistant", &format!("Error: {}", e));
                                                        app.set_status(format!("Chat error: {}", e));
                                                    }
                                                }
                                                app.chat_loading = false;
                                            }
                                            Err(e) => {
                                                app.push_chat_message("assistant", &format!("Config error: {}", e));
                                            }
                                        }
                                    } else {
                                        app.push_chat_message("assistant", "AI not configured. Run: duet config ai --setup");
                                    }
                                } else {
                                    app.push_chat_message("assistant", "Play a track first to chat about it!");
                                }
                            }
                            handled = true;
                        }
                        KeyCode::Up => {
                            app.chat_scroll = app.chat_scroll.saturating_add(1);
                            handled = true;
                        }
                        KeyCode::Down => {
                            app.chat_scroll = app.chat_scroll.saturating_sub(1);
                            handled = true;
                        }
                        KeyCode::Backspace => {
                            app.chat_input.pop();
                        }
                        KeyCode::Char(c) => {
                            app.chat_input.push(c);
                            handled = true;
                        }
                        _ => {}
                    },

                    Panel::Help => {
                        // Any key goes back
                        app.set_panel(Panel::Search);
                    }
                }

                // Player controls available from any panel when playing
                // (skip if the panel already consumed this key, e.g. typing in Search)
                if !handled && player.is_some() {
                    match (code, modifiers) {
                        // Pause / resume
                        (KeyCode::Char(' '), _) => {
                            if let Some(ref p) = player {
                                p.toggle_pause().await.ok();
                                if let Some(ref mut np) = app.now_playing {
                                    np.paused = !np.paused;
                                }
                            }
                        }
                        // Seek ±10s (non-list panels) or Shift+←/→ everywhere
                        (KeyCode::Left, KeyModifiers::NONE) if !matches!(app.panel, Panel::Results | Panel::Queue | Panel::History) => {
                            if let Some(ref p) = player { p.seek(-10.0).await.ok(); }
                        }
                        (KeyCode::Right, KeyModifiers::NONE) if !matches!(app.panel, Panel::Results | Panel::Queue | Panel::History) => {
                            if let Some(ref p) = player { p.seek(10.0).await.ok(); }
                        }
                        (KeyCode::Left, KeyModifiers::SHIFT) => {
                            if let Some(ref p) = player { p.seek(-60.0).await.ok(); }
                        }
                        (KeyCode::Right, KeyModifiers::SHIFT) => {
                            if let Some(ref p) = player { p.seek(60.0).await.ok(); }
                        }
                        // Volume ↑/↓ removed from global block:
                        //   • Search panel: ↑/↓ navigate search history
                        //   • List panels: ↑/↓ navigate list items
                        //   • Use +/- (below) for volume from any panel
                        // Volume via +/- (works from any panel)
                        (KeyCode::Char('+'), _) | (KeyCode::Char('='), _) => {
                            if let Some(ref p) = player {
                                if let Ok(vol) = p.get_volume().await {
                                    let new = (vol + 5).min(100);
                                    p.set_volume(new).await.ok();
                                    app.set_status(format!("Volume: {}%", new));
                                }
                            }
                        }
                        (KeyCode::Char('-'), _) => {
                            if let Some(ref p) = player {
                                if let Ok(vol) = p.get_volume().await {
                                    let new = vol.saturating_sub(5);
                                    p.set_volume(new).await.ok();
                                    app.set_status(format!("Volume: {}%", new));
                                }
                            }
                        }
                        // Speed: ] up, [ down
                        (KeyCode::Char(']'), _) => {
                            if let Some(ref p) = player {
                                let cur = p.get_speed().await.unwrap_or(1.0);
                                let next = crate::util::next_speed_preset(cur, true);
                                p.set_speed(next).await.ok();
                                if let Ok(mut state) = crate::player::state::StateFile::read() {
                                    state.speed = next;
                                    state.write().ok();
                                }
                                app.set_status(format!("Speed: {}x", next));
                            }
                        }
                        (KeyCode::Char('['), _) => {
                            if let Some(ref p) = player {
                                let cur = p.get_speed().await.unwrap_or(1.0);
                                let next = crate::util::next_speed_preset(cur, false);
                                p.set_speed(next).await.ok();
                                if let Ok(mut state) = crate::player::state::StateFile::read() {
                                    state.speed = next;
                                    state.write().ok();
                                }
                                app.set_status(format!("Speed: {}x", next));
                            }
                        }
                        // Next track: n
                        (KeyCode::Char('n'), _) => {
                            if let Some(ref p) = player {
                                p.seek_to(999999.0).await.ok();
                                app.set_status("⏭ Skipped to next");
                            }
                        }
                        // Prev track: p
                        (KeyCode::Char('p'), _) => {
                            if let Some(ref p) = player {
                                p.seek_to(0.0).await.ok();
                                app.set_status("⏮ Restarted / previous");
                            }
                        }
                        // Stop: S (capital)
                        (KeyCode::Char('S'), _) => {
                            if let Some(mut p) = player.take() {
                                p.stop().await.ok();
                                crate::player::state::StateFile::remove().ok();
                            }
                            app.now_playing = None;
                            app.set_status("⏹ Stopped");
                        }
                        // Repeat: r
                        (KeyCode::Char('r'), _) => {
                            if let Ok(mut state) = crate::player::state::StateFile::read() {
                                state.repeat = state.repeat.cycle();
                                let label = state.repeat.label().to_string();
                                state.write().ok();
                                app.set_status(format!("Repeat: {}", label));
                            }
                        }
                        // Shuffle: z
                        (KeyCode::Char('z'), _) => {
                            if let Ok(mut state) = crate::player::state::StateFile::read() {
                                state.shuffle = !state.shuffle;
                                let on = state.shuffle;
                                state.write().ok();
                                app.set_status(format!("Shuffle: {}", if on { "on 🔀" } else { "off" }));
                            }
                        }
                        // Favorite: f
                        (KeyCode::Char('f'), _) => {
                            if let Some(ref mut np) = app.now_playing {
                                if np.is_fav {
                                    library::favorites::remove_favorite(db, &np.video.id).ok();
                                    np.is_fav = false;
                                    app.set_status("Removed from favorites");
                                } else {
                                    library::favorites::add_favorite(db, &np.video).ok();
                                    np.is_fav = true;
                                    app.set_status("Added to favorites ❤️");
                                }
                            }
                        }
                        // Toggle queue: a
                        (KeyCode::Char('a'), _) => {
                            if let Some(ref mut np) = app.now_playing {
                                if np.in_queue {
                                    library::queue::remove_from_queue_by_video_id(db, &np.video.id).ok();
                                    np.in_queue = false;
                                    // Reload queue panel if visible
                                    app.queue_items = library::queue::get_queue(db).unwrap_or_default();
                                    app.set_status("📋✗ Removed from queue");
                                } else {
                                    match library::queue::add_to_queue(db, &np.video) {
                                        Ok(_) => {
                                            np.in_queue = true;
                                            let len = library::queue::queue_length(db).unwrap_or(0);
                                            // Reload queue panel if visible
                                            app.queue_items = library::queue::get_queue(db).unwrap_or_default();
                                            app.set_status(format!("📋 Added to queue (#{})", len));
                                        }
                                        Err(e) => app.set_status(format!("Queue error: {}", e)),
                                    }
                                }
                            }
                        }
                        // Sleep timer: t (toggle 30min / off)
                        (KeyCode::Char('t'), _) => {
                            use chrono::Utc;
                            if let Ok(mut state) = crate::player::state::StateFile::read() {
                                if state.sleep_deadline.is_some() {
                                    state.sleep_deadline = None;
                                    state.write().ok();
                                    app.set_status("😴 Sleep timer cancelled");
                                } else {
                                    let deadline = Utc::now() + chrono::Duration::minutes(30);
                                    state.sleep_deadline = Some(deadline);
                                    state.write().ok();
                                    app.set_status(format!("😴 Sleep in 30min ({})", deadline.format("%H:%M")));
                                }
                            }
                        }
                        // Chat: c (hint — terminal only)
                        (KeyCode::Char('c'), _) => {
                            app.set_status("💬 Chat: quit TUI then run: duet chat");
                        }
                        _ => {}
                    }
                }
            }
        }

        // ─── Sleep timer check ───────────────────────────────────
        if let Ok(state) = crate::player::state::StateFile::read() {
            if let Some(deadline) = state.sleep_deadline {
                if chrono::Utc::now() >= deadline {
                    // Sleep timer expired — stop playback
                    if let Some(mut p) = player.take() {
                        p.stop().await.ok();
                    }
                    crate::player::state::StateFile::remove().ok();
                    app.now_playing = None;
                    app.set_status("😴 Sleep timer expired — playback stopped");
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    // Cleanup
    if let Some(mut p) = player.take() {
        p.stop().await.ok();
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

// ─── Config Command ───────────────────────────────────────────

async fn cmd_config(action: Option<ConfigAction>, config: &mut Config) -> Result<()> {
    use config_cmd as cc;
    match action {
        None => {
            cc::show_all(config);
        }
        Some(ConfigAction::Ai { setup: true, .. }) => {
            cc::run_ai_wizard(config).await?;
        }
        Some(ConfigAction::Ai {
            setup: false,
            action: None,
        }) => {
            cc::show_ai(config);
        }
        Some(ConfigAction::Ai {
            setup: false,
            action: Some(ai_action),
        }) => match ai_action {
            AiAction::Set {
                provider,
                model,
                api_key,
                api_key_env,
                base_url,
            } => {
                cc::ai_set(config, provider, model, api_key, api_key_env, base_url)?;
            }
            AiAction::AddProfile {
                name,
                provider,
                model,
                api_key,
                api_key_env,
                base_url,
            } => {
                cc::add_profile(
                    config,
                    &name,
                    provider,
                    model,
                    api_key,
                    api_key_env,
                    base_url,
                )?;
            }
            AiAction::RemoveProfile { name } => {
                cc::remove_profile(config, &name)?;
            }
            AiAction::ListProfiles => {
                cc::list_profiles(config);
            }
            AiAction::Test { profile } => {
                cc::run_test(config, profile.as_deref()).await?;
            }
        },
        Some(ConfigAction::Player { action: None }) => {
            cc::show_player(config);
        }
        Some(ConfigAction::Player { action: Some(PlayerAction::Set { volume, search_results, backend }) }) => {
            cc::player_set(config, volume, search_results, backend)?;
        }
        Some(ConfigAction::Youtube { action: None }) => {
            cc::show_youtube(config);
        }
        Some(ConfigAction::Youtube { action: Some(YoutubeAction::Set { format, backend }) }) => {
            cc::youtube_set(config, format, backend)?;
        }
        Some(ConfigAction::Set { key, value }) => {
            cc::set_key(config, &key, &value)?;
        }
        Some(ConfigAction::Get { key }) => {
            cc::get_key(config, &key)?;
        }
        Some(ConfigAction::Path) => {
            cc::show_path();
        }
        Some(ConfigAction::Reset { force }) => {
            cc::reset_config(force)?;
        }
    }
    Ok(())
}
