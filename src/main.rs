mod ai;
mod cli;
mod commands;
mod config;
mod config_cmd;
mod error;
mod interactive;
mod library;
mod media;
mod player;
mod tui;
mod util;

use anyhow::Result;
use clap::Parser;
use colored::*;
use dialoguer::{theme::ColorfulTheme, Select};
use std::io::{self, Write};
use std::time::Instant;

use ai::{ai_chat, fetch_transcript, VideoContext};
use cli::{
    AiAction, Cli, Commands, ConfigAction, FavAction, MediaAction, PlayerAction, PlaylistAction,
    QueueAction,
};
use config::Config;
use interactive::{run_interactive, InteractiveAction};
use library::Database;
use media::{MediaBackend, YtDlp};
use player::{MediaPlayer, MpvPlayer};

#[tokio::main]
async fn main() -> Result<()> {
    // Handle Ctrl+C gracefully
    ctrlc::set_handler(move || {
        let _ = std::fs::remove_file("/tmp/aux-mpv.sock");
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
        Commands::Search {
            query,
            limit,
            source,
        } => {
            cmd_search(&query.join(" "), limit, &source, &config, &db).await?;
        }
        Commands::Play {
            url,
            daemon,
            speed,
            repeat,
        } => {
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
            let remote = crate::player::RemoteSession::connect().map_err(|_| {
                anyhow::anyhow!("No active aux session. Start one with: aux play <url>")
            })?;
            remote.pause().await?;
            println!("  ⏸ Paused");
        }
        Commands::Resume => {
            let remote = crate::player::RemoteSession::connect().map_err(|_| {
                anyhow::anyhow!("No active aux session. Start one with: aux play <url>")
            })?;
            remote.resume().await?;
            println!("  ▶ Resumed");
        }
        Commands::Stop => {
            let remote = crate::player::RemoteSession::connect().map_err(|_| {
                anyhow::anyhow!("No active aux session. Start one with: aux play <url>")
            })?;
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
            let log_path = directories::ProjectDirs::from("", "", "aux")
                .map(|d| d.data_dir().join("daemon.log"))
                .unwrap_or_else(|| std::path::PathBuf::from("/tmp/aux-daemon.log"));
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
        Commands::Chat {
            message,
            model: _,
            profile,
        } => {
            cmd_chat_cli(&config, &db, message, profile.as_deref()).await?;
        }
        Commands::Suggest {
            model: _,
            profile: _,
        } => {
            println!(
                "  {}",
                "Suggest requires a video playing. Use: aux search → play → [c] to chat".dimmed()
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
        Commands::Playlist { action } => {
            cmd_playlist(&db, action, &config).await?;
        }
        Commands::Equalizer { preset } => {
            cmd_equalizer(preset).await?;
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
    video: &media::MediaInfo,
    config: &Config,
    db: &Database,
) -> Result<Option<String>> {
    let yt = YtDlp::new();
    let started_at = Instant::now();

    println!("  {} {}", "▶ Playing:".green().bold(), video.title.bold());
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
    let mut current_video = video.clone();
    let result = loop {
        match run_interactive(
            &mut player,
            &current_video,
            db,
            ai_context.transcript.as_ref(),
        )
        .await?
        {
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

                match run_chat_mode(&mut ai_context, config).await? {
                    Some(InteractiveAction::SearchFromChat(query)) => {
                        player.stop().await?;
                        crossterm::terminal::disable_raw_mode().ok();
                        break Some(query);
                    }
                    _ => {
                        // Restore player UI so user sees context after chat
                        print_player_ui(&current_video);
                    }
                }
            }
            InteractiveAction::SleepStop => {
                // Clear sleep deadline
                if let Ok(mut sf) = crate::player::state::StateFile::read() {
                    sf.sleep_deadline = None;
                    sf.write().ok();
                }
                player.stop().await?;
                println!("\n  {} 😴 Sleep timer — goodnight! 🌙", "Stopped.".dimmed());
                break None;
            }
            InteractiveAction::PlayNext => {
                use crate::library::queue;
                // RepeatAll: re-add current track to queue
                if let Ok(sf) = crate::player::state::StateFile::read() {
                    if sf.repeat == crate::player::RepeatMode::All {
                        queue::add_to_queue(db, &current_video).ok();
                    }
                }
                // Pick next: shuffle → random, else sequential
                let shuffle_on = crate::player::state::StateFile::read()
                    .map(|s| s.shuffle)
                    .unwrap_or(false);
                let next_entry = if shuffle_on {
                    let q = queue::get_queue(db).unwrap_or_default();
                    if q.is_empty() {
                        None
                    } else {
                        use std::collections::hash_map::DefaultHasher;
                        use std::hash::{Hash, Hasher};
                        let mut hasher = DefaultHasher::new();
                        std::time::SystemTime::now().hash(&mut hasher);
                        let idx = (hasher.finish() as usize) % q.len();
                        let entry = q[idx].clone();
                        queue::remove_from_queue(db, entry.id).ok();
                        Some(entry)
                    }
                } else {
                    queue::pop_next(db).unwrap_or(None)
                };

                if let Some(entry) = next_entry {
                    println!("\n  {} {}", "⏭ Next:".green().bold(), entry.title.bold());
                    match yt.get_stream_url(&entry.url).await {
                        Ok(stream) => {
                            player.load(&stream.audio_url).await?;
                            current_video = crate::media::MediaInfo {
                                id: entry.video_id.clone(),
                                title: entry.title.clone(),
                                channel: entry.channel.clone(),
                                url: entry.url.clone(),
                                duration: entry.duration_secs.map(|d| d as f64),
                                view_count: None,
                                thumbnail: None,
                                description: None,
                                source: crate::media::Source::default(),
                                extractor_key: None,
                            };
                            let transcript =
                                crate::ai::transcript::fetch_transcript(&current_video.url)
                                    .await
                                    .unwrap_or(None);
                            ai_context = VideoContext::new(current_video.clone(), transcript);
                            library::history::add_to_history(db, &current_video, 0).ok();
                            print_player_ui_inline(&entry.title, entry.channel.as_deref());
                        }
                        Err(e) => {
                            println!("  {} {}", "Failed:".red(), e);
                        }
                    }
                } else {
                    player.stop().await?;
                    println!("\n  {} Queue finished 🎵", "⏹".dimmed());
                    break None;
                }
            }
            // SearchFromChat is only returned from run_chat_mode, not run_interactive
            InteractiveAction::SearchFromChat(_) => unreachable!(),
        }
    };

    // Record to history
    let listened_secs = started_at.elapsed().as_secs();
    library::history::add_to_history(db, &current_video, listened_secs)?;

    Ok(result)
}

// ─── Player UI header ────────────────────────────────────────

/// Print (or re-print) the now-playing header and keybind legend.
/// Called at initial playback start and again after returning from chat mode.
fn print_player_ui(video: &media::MediaInfo) {
    use colored::Colorize;
    let term_w = crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(80);
    let divider = "─".repeat(term_w.min(80));
    println!();
    println!(
        "  {} {}",
        "▶ Now playing:".green().bold(),
        video.title.bold()
    );
    println!(
        "  {} {}",
        "🎵".dimmed(),
        video.channel.as_deref().unwrap_or("Unknown").dimmed()
    );
    println!("  {}", divider.dimmed());
    println!(
        "  {} pause  {} seek  {} vol  {} speed  {} repeat  {} shuf  {} eq  {} fav  {} queue  {} sleep  {} search  {} chat  {} quit",
        "[spc]".cyan(), "[←→]".cyan(), "[↑↓]".cyan(), "[+/-]".cyan(),
        "[r]".cyan(), "[x]".cyan(), "[e]".cyan(), "[f]".cyan(),
        "[a]".cyan(), "[t]".cyan(), "[s]".cyan(), "[c]".cyan(), "[q]".cyan(),
    );
    println!("  {}", divider.dimmed());
    println!();
}

/// Compact UI for auto-play transitions (no full re-print)
fn print_player_ui_inline(title: &str, channel: Option<&str>) {
    use colored::Colorize;
    println!(
        "  {} {}  ·  {}",
        "▶".green().bold(),
        title.bold(),
        channel.unwrap_or("Unknown").dimmed(),
    );
}

// ─── Chat ────────────────────────────────────────────────────

/// TUI chat mode: returns `Some(InteractiveAction)` if an action needs to
/// propagate back to the play loop (e.g. SearchFromChat), or `None` to resume.
async fn run_chat_mode(
    context: &mut VideoContext,
    config: &Config,
) -> Result<Option<InteractiveAction>> {
    println!(
        "\n  {} {} {}\n",
        "💬 Chat mode".bold().cyan(),
        "(playing:".dimmed(),
        format!("{})", context.video.title).dimmed()
    );

    if config.ai.is_none() {
        println!(
            "  {}",
            "⚠️  AI not configured. Run: aux config ai --setup".yellow()
        );
        println!();
        return Ok(None);
    }

    // Resolve AI config (default profile, no overrides)
    let resolved = config.ai.as_ref().unwrap().resolve(None)?;

    println!(
        "  {} Esc to exit chat · /quit or /q to quit · try \"tăng volume\" or \"tìm bài lofi\"\n",
        "💡".dimmed()
    );

    loop {
        let input = read_chat_input()?;

        let input = match input {
            None => break,
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
            Ok(chat_response) => {
                spinner.finish_and_clear();

                // Handle actions if present
                if chat_response.action.is_empty() {
                    // Normal conversational reply
                    println!("  {} {}\n", "🤖:".bold().cyan(), chat_response.message);
                } else {
                    println!("  {} {}\n", "🤖:".bold().cyan(), chat_response.message);
                    for action in &chat_response.action {
                        // Search is special — break out to the play loop
                        if let ai::AiAction::Search { ref query, .. } = action {
                            println!("  {} Chat ended.\n", "👋".dimmed());
                            return Ok(Some(InteractiveAction::SearchFromChat(
                                query.clone(),
                            )));
                        }

                        if let Err(e) = ai::execute_action(action).await {
                            println!(
                                "  {}",
                                format!("(⚠️ action failed: {})", e).red()
                            );
                        }
                    }
                }
            }
            Err(e) => {
                spinner.finish_and_clear();
                println!(
                    "  {} {}\n",
                    "🤖:".bold().cyan(),
                    format!("Error: {}", e).red()
                );
            }
        }
    }

    println!("  {} Chat ended.\n", "👋".dimmed());

    Ok(None)
}

/// CLI `aux chat` handler — single-shot or interactive loop.
async fn cmd_chat_cli(
    config: &Config,
    db: &Database,
    message: Vec<String>,
    profile: Option<&str>,
) -> Result<()> {
    if config.ai.is_none() {
        println!(
            "  {}",
            "⚠️  AI not configured. Run: aux config ai --setup".yellow()
        );
        return Ok(());
    }

    let resolved = config.ai.as_ref().unwrap().resolve(profile)?;

    // Build a VideoContext from StateFile if a session is active,
    // otherwise create an empty context so search/play commands still work.
    let mut context = match crate::player::state::StateFile::read() {
        Ok(s) => VideoContext::new(s.video, None),
        Err(_) => {
            let empty_video = crate::media::MediaInfo {
                id: String::new(),
                title: String::new(),
                channel: None,
                duration: None,
                view_count: None,
                thumbnail: None,
                description: None,
                url: String::new(),
                source: crate::media::Source::default(),
                extractor_key: None,
            };
            VideoContext::new(empty_video, None)
        }
    };

    if message.is_empty() {
        // Interactive loop
        let header_ctx = if context.video.id.is_empty() {
            "no track playing".to_string()
        } else {
            format!("playing: {}", context.video.title)
        };
        println!(
            "\n  {} {}",
            "💬 Chat".bold().cyan(),
            format!("({})", header_ctx).dimmed(),
        );
        println!();
        println!(
            "  {} type a message, /quit to exit · try \"tăng volume\" or \"tìm bài lofi\"\n",
            "💡".dimmed()
        );

        loop {
            let input = read_chat_input()?;
            let input = match input {
                None => break,
                Some(s) => s,
            };
            let input = input.trim().to_string();
            if input.is_empty() {
                continue;
            }
            if input == "/quit" || input == "/q" || input == "exit" {
                break;
            }
            process_chat_message(&mut context, &input, &resolved, config, db).await;
        }
        println!("  {} Chat ended.\n", "👋".dimmed());
    } else {
        // Single-shot
        let msg = message.join(" ");
        process_chat_message(&mut context, &msg, &resolved, config, db).await;
    }

    Ok(())
}

/// Process a single chat message: call AI, execute action, print response.
async fn process_chat_message(
    context: &mut VideoContext,
    input: &str,
    resolved: &crate::config::ResolvedAiConfig,
    config: &Config,
    db: &Database,
) {
    let spinner = indicatif::ProgressBar::new_spinner();
    spinner.set_style(
        indicatif::ProgressStyle::with_template("  🤖:{spinner:.cyan} {msg}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
    );
    spinner.set_message("Thinking...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(80));

    match ai_chat(context, input, resolved).await {
        Ok(chat_response) => {
            spinner.finish_and_clear();

            if chat_response.action.is_empty() {
                println!("  {} {}\n", "🤖:".bold().cyan(), chat_response.message);
            } else {
                println!("  {} {}\n", "🤖:".bold().cyan(), chat_response.message);
                for action in &chat_response.action {
                    // Search from CLI: trigger cmd_search directly
                    if let ai::AiAction::Search { ref query, ref source } = action {
                        let resolved_source = source
                            .as_deref()
                            .unwrap_or(&config.media.default_source);
                        if let Err(e) =
                            cmd_search(query, config.player.search_results, resolved_source, config, db).await
                        {
                            println!("  {} {}\n", "❌".red(), e);
                        }
                        continue;
                    }

                    if let Err(e) = ai::execute_action(action).await {
                        println!(
                            "  {}",
                            format!("(⚠️ {})", e).red()
                        );
                    }
                }
            }
        }
        Err(e) => {
            spinner.finish_and_clear();
            println!(
                "  {} {}\n",
                "🤖:".bold().cyan(),
                format!("Error: {}", e).red()
            );
        }
    }
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

async fn cmd_search(
    query: &str,
    limit: usize,
    source_str: &str,
    config: &Config,
    db: &Database,
) -> Result<()> {
    use colored::Colorize;
    use std::collections::HashSet;

    let source = media::Source::from_str_arg(source_str)
        .or_else(|| media::Source::from_str_arg(&config.media.default_source))
        .unwrap_or(media::Source::YouTube);

    let mut current_query = query.to_string();
    let page_size = if limit > 0 {
        limit
    } else {
        config.player.search_results
    };

    loop {
        println!(
            "\n  {} {} {}{}",
            "🔍 Searching:".green().bold(),
            format!("[{}]", source.display_name()).dimmed(),
            current_query.bold(),
            "\n"
        );

        let yt = YtDlp::new();
        // Fetch a larger batch for local pagination (up to 5 pages)
        let max_fetch = page_size * 5;
        let all_results = yt.search(&current_query, max_fetch, &source).await?;
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
                        .map(|d| media::types::format_duration(d as u64))
                        .unwrap_or_else(|| "LIVE".to_string());
                    let channel = v.channel.as_deref().unwrap_or("Unknown");
                    let fav = if fav_ids.contains(&v.id) {
                        "❤️ "
                    } else {
                        ""
                    };
                    let queue = if queue_ids.contains(&v.id) {
                        "📋"
                    } else {
                        ""
                    };
                    format!(
                        "{}. {}{}{} · {} · {}",
                        global_idx, fav, queue, v.title, channel, duration
                    )
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

    let video = if media::is_direct_url(url) {
        // Direct URL or video ID — fetch metadata without going through ytsearch:
        yt.fetch_info(url).await?
    } else {
        // Keyword query — use search
        let results = yt.search(url, 1, &media::Source::YouTube).await?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No results for: {}", url))?
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
        style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor},
        terminal::{
            disable_raw_mode, enable_raw_mode, size, Clear, ClearType, EnterAlternateScreen,
            LeaveAlternateScreen,
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
            SetForegroundColor(Color::DarkGreen),
            Print(format!(
                "\r\n  ✦ Results for: \x1b[0;1m{}\x1b[0;32m  (page {}/{})\r\n",
                query,
                page.current + 1,
                page.total,
            )),
            ResetColor,
            SetForegroundColor(Color::DarkGrey),
            Print(format!("  {}\r\n", divider)),
            ResetColor,
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
                queue!(io::stdout(), Print(format!("    {}\r\n", display)),)?;
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
            SetForegroundColor(Color::DarkGrey),
            Print(format!("\r\n  {}\r\n", divider)),
            ResetColor,
            Print(format!("  {}{}\r\n", nav_parts.join("   "), counter,)),
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
    let term_w = crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(80);
    let divider = "─".repeat(term_w.min(70));
    let entries = if today {
        println!("\n  {} {}", "📅".bold(), "Today's Listening History".bold());
        library::history::get_today_history(db)?
    } else {
        println!("\n  {} {}", "📜".bold(), "Play History".bold());
        library::history::get_history(db, limit)?
    };
    println!("  {}", divider.dimmed());

    if entries.is_empty() {
        println!(
            "  {} {}",
            "🎵".dimmed(),
            "No history yet. Play some music!".dimmed()
        );
        println!();
        return Ok(());
    }

    for (i, entry) in entries.iter().enumerate() {
        let channel = entry.channel.as_deref().unwrap_or("Unknown");
        let duration = entry
            .duration_secs
            .map(|d| media::types::format_duration(d as u64))
            .unwrap_or_else(|| "?".to_string());
        let listened = media::types::format_duration(entry.listened_secs as u64);
        let when = entry
            .played_at
            .split('T')
            .next()
            .unwrap_or(&entry.played_at);

        println!(
            "  {} {}  ·  {}  ·  {} {}  ·  {}",
            format!("{:>3}.", i + 1).dimmed(),
            entry.title.bold(),
            channel.dimmed(),
            "⏱".dimmed(),
            format!("{}/{}", listened, duration).green(),
            when.dimmed()
        );
    }
    println!("  {}", divider.dimmed());
    println!(
        "  {} {} tracks  ·  {} total listened",
        "∑".dimmed(),
        entries.len(),
        media::types::format_duration(
            entries
                .iter()
                .map(|e| e.listened_secs.max(0) as u64)
                .sum::<u64>()
        )
        .green(),
    );
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
                    "  {} {}",
                    "└".dimmed(),
                    "No favorites yet. Press [f] while playing to add one!".dimmed()
                );
                println!();
                return Ok(());
            }

            let items: Vec<String> = entries
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let duration = v
                        .duration_secs
                        .map(|d| media::types::format_duration(d as u64))
                        .unwrap_or_else(|| "?".to_string());
                    let channel = v.channel.as_deref().unwrap_or("Unknown");
                    format!("{}. ❤ {}  ·  {}  ·  {}", i + 1, v.title, channel, duration)
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
            let results = yt.search(&url, 1, &media::Source::YouTube).await?;
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
                    "  {} {}",
                    "└".dimmed(),
                    "Queue is empty. Press [a] while playing to add tracks!".dimmed()
                );
                println!();
                return Ok(());
            }

            let term_w = crossterm::terminal::size()
                .map(|(w, _)| w as usize)
                .unwrap_or(80);
            let divider = "─".repeat(term_w.min(70));

            for (i, entry) in entries.iter().enumerate() {
                let channel = entry.channel.as_deref().unwrap_or("Unknown");
                let duration = entry
                    .duration_secs
                    .map(|d| media::types::format_duration(d as u64))
                    .unwrap_or_else(|| "?".to_string());

                let prefix = if i == 0 {
                    format!("{}", "▶".green())
                } else {
                    format!("{:>3}", i + 1)
                };

                println!(
                    "  {}. {}  ·  {}  ·  {}",
                    prefix,
                    entry.title.bold(),
                    channel.dimmed(),
                    duration.dimmed()
                );
            }
            println!("  {}", divider.dimmed());
            println!("  {} {} tracks in queue", "∑".dimmed(), entries.len());
            println!();
        }
        Some(QueueAction::Add { url }) => {
            let yt = YtDlp::new();
            let results = yt.search(&url, 1, &media::Source::YouTube).await?;
            if let Some(video) = results.first() {
                if library::queue::add_to_queue(db, video)? {
                    let len = library::queue::queue_length(db)?;
                    println!(
                        "  {} {} added to queue (#{})",
                        "📋",
                        video.title.bold(),
                        len
                    );
                } else {
                    println!(
                        "  {} {} is already in queue — skipped",
                        "⚠️",
                        video.title.dimmed()
                    );
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
// ─── Playlists ────────────────────────────────────────────────

async fn cmd_playlist(
    db: &Database,
    action: Option<PlaylistAction>,
    config: &Config,
) -> Result<()> {
    use crate::library::playlist;

    match action {
        None | Some(PlaylistAction::List) => {
            let pls = playlist::list_playlists(db)?;
            println!("\n  {} {}\n", "🎶".bold(), "Playlists".bold());
            if pls.is_empty() {
                println!(
                    "  {}",
                    "No playlists yet. Create one: aux playlist create <name>".dimmed()
                );
                return Ok(());
            }
            for (i, pl) in pls.iter().enumerate() {
                println!(
                    "  {} {} ({} tracks)",
                    format!("{}.", i + 1).dimmed(),
                    pl.name.bold(),
                    pl.item_count,
                );
            }
            println!();
        }
        Some(PlaylistAction::Create { name }) => match playlist::create_playlist(db, &name) {
            Ok(_) => println!("  {} Created playlist: {}", "🎶", name.bold()),
            Err(_) => println!("  {} Playlist '{}' already exists", "⚠️", name),
        },
        Some(PlaylistAction::Delete { name }) => {
            if playlist::delete_playlist(db, &name)? {
                println!("  {} Deleted playlist: {}", "🗑️", name);
            } else {
                println!("  {} Playlist '{}' not found", "⚠️", name);
            }
        }
        Some(PlaylistAction::Show { name }) => match playlist::get_playlist_items(db, &name) {
            Ok(items) => {
                println!(
                    "\n  {} {} ({} tracks)\n",
                    "🎶".bold(),
                    name.bold(),
                    items.len()
                );
                for (i, item) in items.iter().enumerate() {
                    let ch = item.channel.as_deref().unwrap_or("Unknown");
                    println!(
                        "  {} {} — {}",
                        format!("{}.", i + 1).dimmed(),
                        item.title.bold(),
                        ch.dimmed(),
                    );
                }
                println!();
            }
            Err(_) => println!("  {} Playlist '{}' not found", "⚠️", name),
        },
        Some(PlaylistAction::Add { name, url }) => {
            let yt = YtDlp::new();
            let results = yt.search(&url, 1, &media::Source::YouTube).await?;
            if let Some(video) = results.first() {
                match playlist::add_to_playlist(db, &name, video) {
                    Ok(true) => println!("  {} {} added to '{}'", "🎶", video.title.bold(), name),
                    Ok(false) => println!("  {} Already in playlist", "⚠️"),
                    Err(e) => println!("  {} Error: {}", "❌", e),
                }
            }
        }
        Some(PlaylistAction::Remove { name, video_id }) => {
            if playlist::remove_from_playlist(db, &name, &video_id)? {
                println!("  {} Removed from '{}'", "🗑️", name);
            } else {
                println!("  {} Video not found in playlist", "⚠️");
            }
        }
        Some(PlaylistAction::Play { name }) => {
            // Clear queue, load playlist items, then play first
            library::queue::clear_queue(db)?;
            match playlist::load_playlist_to_queue(db, &name) {
                Ok(count) if count > 0 => {
                    println!(
                        "  {} Loaded {} tracks from '{}' into queue",
                        "🎶",
                        count,
                        name.bold()
                    );
                    // Play first item
                    if let Some(entry) = library::queue::pop_next(db)? {
                        println!("  {} Playing: {}", "▶".green(), entry.title.bold());
                        cmd_play(&entry.url, config, db).await?;
                    }
                }
                Ok(_) => println!("  {} Playlist '{}' is empty", "⚠️", name),
                Err(e) => println!("  {} Error: {}", "❌", e),
            }
        }
    }
    Ok(())
}

// ─── Equalizer ────────────────────────────────────────────────

/// EQ preset name → mpv audio filter string
pub fn eq_preset_filter(name: &str) -> Option<&'static str> {
    match name {
        "flat" => Some(""),
        "bass-boost" => Some("superequalizer=1b=5:2b=4:3b=3:4b=1"),
        "vocal" => Some("superequalizer=6b=3:7b=4:8b=3:9b=2"),
        "treble" => Some("superequalizer=8b=3:9b=4:10b=5:11b=4"),
        "loudness" => Some("superequalizer=1b=4:2b=3:9b=3:10b=4"),
        _ => None,
    }
}

pub fn eq_preset_names() -> &'static [&'static str] {
    &["flat", "bass-boost", "vocal", "treble", "loudness"]
}

async fn cmd_equalizer(preset: Option<String>) -> Result<()> {
    match preset {
        None => {
            // Show current EQ and available presets
            let current = crate::player::state::StateFile::read()
                .ok()
                .and_then(|s| s.eq_preset)
                .unwrap_or_else(|| "flat".to_string());
            println!("\n  {} {}\n", "🎛️".bold(), "Equalizer".bold());
            for name in eq_preset_names() {
                let marker = if *name == current { "▶" } else { " " };
                println!("  {} {}", marker.green(), name.bold());
            }
            println!("\n  Usage: aux eq <preset>");
        }
        Some(name) => {
            if eq_preset_filter(&name).is_some() {
                // Save to state file
                if let Ok(mut state) = crate::player::state::StateFile::read() {
                    state.eq_preset = Some(name.clone());
                    state.write().ok();
                }
                // Apply to running player
                let player = crate::player::mpv::MpvPlayer::connect_existing();
                if let Ok(p) = player {
                    let filter = eq_preset_filter(&name).unwrap();
                    if filter.is_empty() {
                        p.set_audio_filter("").await.ok();
                    } else {
                        p.set_audio_filter(filter).await.ok();
                    }
                }
                println!("  {} EQ set to: {}", "🎛️", name.bold());
            } else {
                println!(
                    "  {} Unknown preset: '{}'. Available: {:?}",
                    "❌",
                    name,
                    eq_preset_names()
                );
            }
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
    use tui::app::{App, Panel};
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
    let _yt = YtDlp::new();
    let mut player: Option<MpvPlayer> = None;
    let mut ai_context: Option<VideoContext> = None;
    let mut last_position_save = std::time::Instant::now();
    let mut last_mpv_poll = std::time::Instant::now();
    let mut last_state_read = std::time::Instant::now();

    // Pre-load saved playback positions for UX indicators
    app.saved_positions = library::playback_position::get_all_positions(db).unwrap_or_default();

    // Background task handles for non-blocking I/O
    let mut pending_transcript: Option<
        tokio::task::JoinHandle<Option<crate::ai::transcript::Transcript>>,
    > = None;
    let mut pending_transcript_video: Option<crate::media::MediaInfo> = None;
    let mut pending_search: Option<(
        tokio::task::JoinHandle<anyhow::Result<Vec<crate::media::MediaInfo>>>,
        String,
    )> = None;
    let mut pending_stream: Option<(
        tokio::task::JoinHandle<anyhow::Result<crate::media::types::StreamUrl>>,
        crate::media::MediaInfo,
        bool, // is_fav
        bool, // in_queue
    )> = None;
    // Non-blocking AI chat: spawned task returns (context, result) so we can
    // restore ai_context without Clone. While the task runs, ai_context is None.
    let mut pending_chat: Option<
        tokio::task::JoinHandle<(
            ai::VideoContext,
            anyhow::Result<ai::ChatResponse>,
        )>,
    > = None;
    // Queued AI actions — when a pipeline like [Search, PlayResult] is returned,
    // Search executes immediately and PlayResult is queued here to run after
    // search results arrive.
    let mut pending_ai_actions: Vec<ai::AiAction> = Vec::new();

    loop {
        // Draw
        terminal.draw(|frame| {
            ui::draw(frame, &app);
        })?;

        // ── Poll background tasks ──────────────────────────────────────────
        // Transcript completion
        if let Some(ref handle) = pending_transcript {
            if handle.is_finished() {
                let handle = pending_transcript.take().unwrap();
                let transcript = handle.await.ok().flatten();
                app.transcript = transcript.clone();
                app.lyrics_scroll = 0;
                app.lyrics_auto_scroll = true;
                if let Some(video) = pending_transcript_video.take() {
                    let mut new_ctx = VideoContext::new(video, transcript);
                    // Preserve chat history, search results, and position from previous context
                    if let Some(old_ctx) = ai_context.take() {
                        new_ctx.chat_history = old_ctx.chat_history;
                        new_ctx.search_results = old_ctx.search_results;
                        new_ctx.current_position = old_ctx.current_position;
                    } else {
                        // ai_context was taken by an in-flight chat task;
                        // recover position from the now-playing state.
                        if let Some(ref np) = app.now_playing {
                            new_ctx.current_position =
                                std::time::Duration::from_secs(np.position_secs);
                        }
                    }
                    ai_context = Some(new_ctx);
                    // Don't clear chat messages — keep conversation flowing
                }
            }
        }

        // AI chat completion (non-blocking)
        if let Some(ref handle) = pending_chat {
            if handle.is_finished() {
                let handle = pending_chat.take().unwrap();
                match handle.await {
                    Ok((mut ctx, result)) => {
                        // Race condition guard: if transcript arrived while this chat
                        // was in-flight (ai_context was taken), pick it up now — once.
                        if ctx.transcript.is_none() {
                            if let Some(ref t) = app.transcript {
                                ctx.transcript = Some(t.clone());
                            }
                        }
                        ai_context = Some(ctx);
                        match result {
                            Ok(chat_response) => {
                                let actions = chat_response.action;
                                let mut action_err: Option<anyhow::Error> = None;
                                let mut player_dead = false;

                                // Debug: log detected actions
                                if !actions.is_empty() {
                                    let action_names: Vec<String> = actions.iter().map(|a| format!("{:?}", a)).collect();
                                    app.set_status(format!("🤖 Actions: {}", action_names.join(" → ")));
                                }

                                // Process actions sequentially; queue remainder
                                // after async-barrier actions like Search.
                                let mut action_iter = actions.into_iter();
                                while let Some(action) = action_iter.next() {
                                    match action {
                                        ai::AiAction::Search { ref query, ref source } => {
                                            let resolved_source = source
                                                .as_deref()
                                                .and_then(crate::media::Source::from_str_arg)
                                                .unwrap_or_else(|| app.search_source.clone());
                                            // Update TUI search source to match
                                            app.search_source = resolved_source.clone();
                                            let q = query.clone();
                                            app.search_input = query.clone();
                                            let source_label = resolved_source.display_name();
                                            app.set_status(format!("🔍 AI searching on {}: {}", source_label, query));
                                            let fetch_count = app.search_page_size * 5;
                                            let source = resolved_source;
                                            pending_search = Some((
                                                tokio::spawn(async move {
                                                    let yt = YtDlp::new();
                                                    use crate::media::MediaBackend;
                                                    yt.search(&q, fetch_count, &source).await
                                                }),
                                                query.clone(),
                                            ));
                                            // Queue remaining actions for after search completes
                                            pending_ai_actions = action_iter.collect();
                                            break;
                                        }
                                        ai::AiAction::PlayResult { index } => {
                                            // Pick from current search results (1-based → 0-based)
                                            if index == 0 || index > app.search_results.len() {
                                                action_err = Some(anyhow::anyhow!(
                                                    "Invalid result index {} (have {} results)",
                                                    index,
                                                    app.search_results.len()
                                                ));
                                            } else {
                                                let video = app.search_results[index - 1].clone();
                                                let url = video.url.clone();
                                                let is_fav = library::favorites::is_favorite(db, &video.id).unwrap_or(false);
                                                let in_queue = app.queue_items.iter().any(|q| q.video_id == video.id);
                                                app.set_status(format!("▶ Playing: {}", video.title));
                                                pending_stream = Some((
                                                    tokio::spawn(async move {
                                                        let yt = YtDlp::new();
                                                        use crate::media::MediaBackend;
                                                        yt.get_stream_url(&url).await
                                                    }),
                                                    video,
                                                    is_fav,
                                                    in_queue,
                                                ));
                                            }
                                        }
                                        ai::AiAction::PlayRandom => {
                                            if app.search_results.is_empty() {
                                                action_err = Some(anyhow::anyhow!(
                                                    "No search results to pick from"
                                                ));
                                            } else {
                                                use std::collections::hash_map::DefaultHasher;
                                                use std::hash::{Hash, Hasher};
                                                let mut hasher = DefaultHasher::new();
                                                std::time::SystemTime::now().hash(&mut hasher);
                                                let idx = (hasher.finish() as usize) % app.search_results.len();
                                                let video = app.search_results[idx].clone();
                                                let url = video.url.clone();
                                                let is_fav = library::favorites::is_favorite(db, &video.id).unwrap_or(false);
                                                let in_queue = app.queue_items.iter().any(|q| q.video_id == video.id);
                                                app.set_status(format!("🎲 Random: {}", video.title));
                                                pending_stream = Some((
                                                    tokio::spawn(async move {
                                                        let yt = YtDlp::new();
                                                        use crate::media::MediaBackend;
                                                        yt.get_stream_url(&url).await
                                                    }),
                                                    video,
                                                    is_fav,
                                                    in_queue,
                                                ));
                                            }
                                        }
                                        ai::AiAction::AddFavorite => {
                                            if let Some(ref np) = app.now_playing {
                                                match library::favorites::add_favorite(db, &np.video) {
                                                    Ok(_) => {
                                                        app.fav_items = library::favorites::get_favorites(db).unwrap_or_default();
                                                        app.set_status(format!("❤️ Added: {}", np.video.title));
                                                    }
                                                    Err(e) => action_err = Some(e),
                                                }
                                            } else {
                                                action_err = Some(anyhow::anyhow!("No track playing"));
                                            }
                                        }
                                        ai::AiAction::RemoveFavorite => {
                                            if let Some(ref np) = app.now_playing {
                                                match library::favorites::remove_favorite(db, &np.video.id) {
                                                    Ok(_) => {
                                                        app.fav_items = library::favorites::get_favorites(db).unwrap_or_default();
                                                        app.set_status(format!("💔 Removed: {}", np.video.title));
                                                    }
                                                    Err(e) => action_err = Some(e),
                                                }
                                            } else {
                                                action_err = Some(anyhow::anyhow!("No track playing"));
                                            }
                                        }
                                        ai::AiAction::AddToQueue => {
                                            if let Some(ref np) = app.now_playing {
                                                match library::queue::add_to_queue(db, &np.video) {
                                                    Ok(added) => {
                                                        app.queue_items = library::queue::get_queue(db).unwrap_or_default();
                                                        if added {
                                                            app.set_status(format!("📋 Queued: {}", np.video.title));
                                                        } else {
                                                            app.set_status("Already in queue");
                                                        }
                                                    }
                                                    Err(e) => action_err = Some(e),
                                                }
                                            } else {
                                                action_err = Some(anyhow::anyhow!("No track playing"));
                                            }
                                        }
                                        ai::AiAction::ClearQueue => {
                                            match library::queue::clear_queue(db) {
                                                Ok(count) => {
                                                    app.queue_items = Vec::new();
                                                    app.set_status(format!("🗑 Cleared {} items from queue", count));
                                                }
                                                Err(e) => action_err = Some(e),
                                            }
                                        }
                                        ai::AiAction::ShowPanel { ref panel } => {
                                            let target = match panel.as_str() {
                                                "queue" => Some(Panel::Queue),
                                                "favorites" | "favourite" | "fav" => Some(Panel::Favorites),
                                                "history" => Some(Panel::History),
                                                "lyrics" => Some(Panel::Lyrics),
                                                "search" | "results" => Some(Panel::Results),
                                                "chat" => Some(Panel::Chat),
                                                "playlists" | "playlist" => Some(Panel::Playlists),
                                                _ => None,
                                            };
                                            if let Some(p) = target {
                                                app.set_panel(p);
                                            } else {
                                                action_err = Some(anyhow::anyhow!("Unknown panel: {}", panel));
                                            }
                                        }
                                        ai::AiAction::CreatePlaylist { ref name } => {
                                            match library::playlist::create_playlist(db, name) {
                                                Ok(_) => {
                                                    app.playlist_list = library::playlist::list_playlists(db).unwrap_or_default();
                                                    app.set_status(format!("🎶 Created playlist: {}", name));
                                                }
                                                Err(e) => action_err = Some(e),
                                            }
                                        }
                                        ai::AiAction::DeletePlaylist { ref name } => {
                                            match library::playlist::delete_playlist(db, name) {
                                                Ok(true) => {
                                                    app.playlist_list = library::playlist::list_playlists(db).unwrap_or_default();
                                                    app.set_status(format!("🗑 Deleted playlist: {}", name));
                                                }
                                                Ok(false) => {
                                                    action_err = Some(anyhow::anyhow!("Playlist '{}' not found", name));
                                                }
                                                Err(e) => action_err = Some(e),
                                            }
                                        }
                                        ai::AiAction::AddToPlaylist { ref playlist } => {
                                            if let Some(ref np) = app.now_playing {
                                                match library::playlist::add_to_playlist(db, playlist, &np.video) {
                                                    Ok(true) => {
                                                        app.set_status(format!("➕ Added to {}: {}", playlist, np.video.title));
                                                    }
                                                    Ok(false) => {
                                                        app.set_status(format!("Already in playlist {}", playlist));
                                                    }
                                                    Err(e) => action_err = Some(e),
                                                }
                                            } else {
                                                action_err = Some(anyhow::anyhow!("No track playing to add"));
                                            }
                                        }
                                        ai::AiAction::PlayPlaylist { ref name } => {
                                            match library::playlist::load_playlist_to_queue(db, name) {
                                                Ok(count) if count > 0 => {
                                                    app.queue_items = library::queue::get_queue(db).unwrap_or_default();
                                                    // Pop first item and start playing
                                                    if let Some(entry) = library::queue::pop_next(db).unwrap_or(None) {
                                                        app.queue_items = library::queue::get_queue(db).unwrap_or_default();
                                                        let video = crate::media::MediaInfo {
                                                            id: entry.video_id.clone(),
                                                            title: entry.title.clone(),
                                                            channel: entry.channel.clone(),
                                                            url: entry.url.clone(),
                                                            duration: entry.duration_secs.map(|d| d as f64),
                                                            view_count: None,
                                                            thumbnail: None,
                                                            description: None,
                                                            source: crate::media::Source::default(),
                                                            extractor_key: None,
                                                        };
                                                        let url = entry.url.clone();
                                                        let is_fav = library::favorites::is_favorite(db, &entry.video_id).unwrap_or(false);
                                                        app.set_status(format!("🎵 Playing playlist {}: {}", name, entry.title));
                                                        pending_stream = Some((
                                                            tokio::spawn(async move {
                                                                let yt = YtDlp::new();
                                                                use crate::media::MediaBackend;
                                                                yt.get_stream_url(&url).await
                                                            }),
                                                            video,
                                                            is_fav,
                                                            false,
                                                        ));
                                                    }
                                                }
                                                Ok(_) => {
                                                    action_err = Some(anyhow::anyhow!("Playlist '{}' is empty", name));
                                                }
                                                Err(e) => action_err = Some(e),
                                            }
                                        }
                                        // All other actions — execute inline
                                        ref other => {
                                            if let Some(ref p) = player {
                                                let r = match other {
                                                    ai::AiAction::SetVolume { value } => {
                                                        let r = p.set_volume(*value).await;
                                                        if r.is_ok() {
                                                            if let Some(ref mut np) = app.now_playing {
                                                                np.volume = *value;
                                                            }
                                                        }
                                                        r
                                                    }
                                                    ai::AiAction::Mute => {
                                                        let r = p.set_volume(0).await;
                                                        if r.is_ok() {
                                                            if let Some(ref mut np) = app.now_playing {
                                                                np.volume = 0;
                                                            }
                                                        }
                                                        r
                                                    }
                                                    ai::AiAction::Pause => {
                                                        let r = p.pause().await;
                                                        if r.is_ok() {
                                                            if let Some(ref mut np) = app.now_playing {
                                                                np.paused = true;
                                                            }
                                                        }
                                                        r
                                                    }
                                                    ai::AiAction::Resume => {
                                                        let r = p.resume().await;
                                                        if r.is_ok() {
                                                            if let Some(ref mut np) = app.now_playing {
                                                                np.paused = false;
                                                            }
                                                        }
                                                        r
                                                    }
                                                    ai::AiAction::Seek { seconds } => {
                                                        p.seek(*seconds).await
                                                    }
                                                    ai::AiAction::Next => {
                                                        // Try to play next in search results first
                                                        let played = if !app.search_results.is_empty() {
                                                            if let Some(ref np) = app.now_playing {
                                                                let cur_idx = app.search_results.iter().position(|v| v.id == np.video.id);
                                                                if let Some(idx) = cur_idx {
                                                                    let next_idx = idx + 1;
                                                                    if next_idx < app.search_results.len() {
                                                                        let video = app.search_results[next_idx].clone();
                                                                        let url = video.url.clone();
                                                                        let is_fav = library::favorites::is_favorite(db, &video.id).unwrap_or(false);
                                                                        let in_queue = app.queue_items.iter().any(|q| q.video_id == video.id);
                                                                        app.set_status(format!("⏭ Next: {}", video.title));
                                                                        pending_stream = Some((
                                                                            tokio::spawn(async move {
                                                                                let yt = YtDlp::new();
                                                                                use crate::media::MediaBackend;
                                                                                yt.get_stream_url(&url).await
                                                                            }),
                                                                            video,
                                                                            is_fav,
                                                                            in_queue,
                                                                        ));
                                                                        true
                                                                    } else { false }
                                                                } else { false }
                                                            } else { false }
                                                        } else { false };
                                                        if played { Ok(()) } else {
                                                            // Fall back to queue-based skip
                                                            p.seek_to(999999.0).await
                                                        }
                                                    }
                                                    ai::AiAction::Prev => {
                                                        // Try to play prev in search results first
                                                        let played = if !app.search_results.is_empty() {
                                                            if let Some(ref np) = app.now_playing {
                                                                let cur_idx = app.search_results.iter().position(|v| v.id == np.video.id);
                                                                if let Some(idx) = cur_idx {
                                                                    if idx > 0 {
                                                                        let video = app.search_results[idx - 1].clone();
                                                                        let url = video.url.clone();
                                                                        let is_fav = library::favorites::is_favorite(db, &video.id).unwrap_or(false);
                                                                        let in_queue = app.queue_items.iter().any(|q| q.video_id == video.id);
                                                                        app.set_status(format!("⏮ Prev: {}", video.title));
                                                                        pending_stream = Some((
                                                                            tokio::spawn(async move {
                                                                                let yt = YtDlp::new();
                                                                                use crate::media::MediaBackend;
                                                                                yt.get_stream_url(&url).await
                                                                            }),
                                                                            video,
                                                                            is_fav,
                                                                            in_queue,
                                                                        ));
                                                                        true
                                                                    } else { false }
                                                                } else { false }
                                                            } else { false }
                                                        } else { false };
                                                        if played { Ok(()) } else {
                                                            // Fall back to restart current track
                                                            p.seek_to(0.0).await
                                                        }
                                                    }
                                                    ai::AiAction::SetSpeed { value } => {
                                                        let speed = value.clamp(0.25, 4.0);
                                                        let r = p.set_speed(speed).await;
                                                        if r.is_ok() {
                                                            if let Some(ref mut np) = app.now_playing {
                                                                np.speed = speed;
                                                            }
                                                            if let Ok(mut state) = crate::player::state::StateFile::read() {
                                                                state.speed = speed;
                                                                state.write().ok();
                                                            }
                                                        }
                                                        r
                                                    }
                                                    ai::AiAction::SetRepeat { mode } => {
                                                        let repeat = match mode.as_str() {
                                                            "one" => crate::player::RepeatMode::One,
                                                            "all" => crate::player::RepeatMode::All,
                                                            _ => crate::player::RepeatMode::Off,
                                                        };
                                                        p.set_loop_file(repeat == crate::player::RepeatMode::One)
                                                            .await
                                                            .ok();
                                                        if let Ok(mut state) = crate::player::state::StateFile::read() {
                                                            state.repeat = repeat;
                                                            state.write().ok();
                                                        }
                                                        Ok(())
                                                    }
                                                    ai::AiAction::ToggleShuffle => {
                                                        if let Ok(mut state) = crate::player::state::StateFile::read() {
                                                            state.shuffle = !state.shuffle;
                                                            state.write().ok();
                                                        }
                                                        Ok(())
                                                    }
                                                    ai::AiAction::SetSleep { minutes } => {
                                                        let deadline = chrono::Utc::now()
                                                            + chrono::Duration::minutes(i64::from(*minutes));
                                                        if let Ok(mut state) = crate::player::state::StateFile::read() {
                                                            state.sleep_deadline = Some(deadline);
                                                            state.write().ok();
                                                        }
                                                        Ok(())
                                                    }
                                                    ai::AiAction::CancelSleep => {
                                                        if let Ok(mut state) = crate::player::state::StateFile::read() {
                                                            state.sleep_deadline = None;
                                                            state.write().ok();
                                                        }
                                                        Ok(())
                                                    }
                                                    _ => unreachable!(),
                                                };
                                                if let Err(e) = r {
                                                    let err_msg = format!("{}", e);
                                                    // Detect dead mpv socket
                                                    if err_msg.contains("connect")
                                                        || err_msg.contains("socket")
                                                        || err_msg.contains("broken pipe")
                                                        || err_msg.contains("Io(")
                                                    {
                                                        player_dead = true;
                                                        action_err = Some(anyhow::anyhow!(
                                                            "Player disconnected. Search and play a new track."
                                                        ));
                                                    } else {
                                                        action_err = Some(e);
                                                    }
                                                }
                                            } else {
                                                // No player — only state-file actions work
                                                match other {
                                                    ai::AiAction::SetSleep { minutes } => {
                                                        let deadline = chrono::Utc::now()
                                                            + chrono::Duration::minutes(i64::from(*minutes));
                                                        if let Ok(mut state) = crate::player::state::StateFile::read() {
                                                            state.sleep_deadline = Some(deadline);
                                                            state.write().ok();
                                                        }
                                                    }
                                                    ai::AiAction::CancelSleep => {
                                                        if let Ok(mut state) = crate::player::state::StateFile::read() {
                                                            state.sleep_deadline = None;
                                                            state.write().ok();
                                                        }
                                                    }
                                                    _ => {
                                                        action_err = Some(anyhow::anyhow!("No track playing"));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // Clean up stale player if socket died
                                if player_dead {
                                    if let Some(mut p) = player.take() {
                                        p.stop().await.ok();
                                    }
                                    app.now_playing = None;
                                    crate::player::state::StateFile::remove().ok();
                                }

                                match action_err {
                                    Some(e) => {
                                        app.push_chat_message(
                                            "assistant",
                                            &format!("{} (⚠️ {})", chat_response.message, e),
                                        );
                                    }
                                    None => {
                                        app.push_chat_message(
                                            "assistant",
                                            &chat_response.message,
                                        );
                                    }
                                }
                                app.set_status("Reply received");
                            }
                            Err(e) => {
                                app.push_chat_message(
                                    "assistant",
                                    &format!("Error: {}", e),
                                );
                                app.set_status(format!("Chat error: {}", e));
                            }
                        }
                    }
                    Err(e) => {
                        app.push_chat_message(
                            "assistant",
                            &format!("Chat task failed: {}", e),
                        );
                    }
                }
                app.chat_loading = false;
            }
        }

        // Search completion
        if let Some((ref handle, _)) = pending_search {
            if handle.is_finished() {
                let (handle, query) = pending_search.take().unwrap();
                match handle.await {
                    Ok(Ok(results)) => {
                        library::search_history::add_search(db, &query).ok();
                        app.search_history =
                            library::search_history::get_searches(db, 100).unwrap_or_default();
                        let total = results.len();
                        app.set_search_results(results);
                        app.set_panel(Panel::Results);
                        app.set_status(format!(
                            "Found {} results (page 1/{})",
                            total,
                            app.search_total_pages()
                        ));

                        // Sync search results to AI context for future reference
                        if let Some(ref mut ctx) = ai_context {
                            ctx.search_results = app
                                .search_results
                                .iter()
                                .map(|v| format!("{}", v))
                                .collect();
                        }

                        // Drain queued AI actions (e.g. PlayResult after Search)
                        if !pending_ai_actions.is_empty() {
                            let queued = std::mem::take(&mut pending_ai_actions);
                            for action in queued {
                                match action {
                                    ai::AiAction::PlayResult { index } => {
                                        if index == 0 || index > app.search_results.len() {
                                            app.push_chat_message(
                                                "assistant",
                                                &format!(
                                                    "⚠️ Couldn't auto-play: index {} out of range (have {} results)",
                                                    index,
                                                    app.search_results.len()
                                                ),
                                            );
                                        } else {
                                            let video = app.search_results[index - 1].clone();
                                            let url = video.url.clone();
                                            let is_fav = library::favorites::is_favorite(db, &video.id).unwrap_or(false);
                                            let in_queue = app.queue_items.iter().any(|q| q.video_id == video.id);
                                            app.set_status(format!("▶ Playing: {}", video.title));
                                            pending_stream = Some((
                                                tokio::spawn(async move {
                                                    let yt = YtDlp::new();
                                                    use crate::media::MediaBackend;
                                                    yt.get_stream_url(&url).await
                                                }),
                                                video,
                                                is_fav,
                                                in_queue,
                                            ));
                                        }
                                    }
                                    ai::AiAction::PlayRandom => {
                                        if !app.search_results.is_empty() {
                                            use std::collections::hash_map::DefaultHasher;
                                            use std::hash::{Hash, Hasher};
                                            let mut hasher = DefaultHasher::new();
                                            std::time::SystemTime::now().hash(&mut hasher);
                                            let idx = (hasher.finish() as usize) % app.search_results.len();
                                            let video = app.search_results[idx].clone();
                                            let url = video.url.clone();
                                            let is_fav = library::favorites::is_favorite(db, &video.id).unwrap_or(false);
                                            let in_queue = app.queue_items.iter().any(|q| q.video_id == video.id);
                                            app.set_status(format!("🎲 Random: {}", video.title));
                                            pending_stream = Some((
                                                tokio::spawn(async move {
                                                    let yt = YtDlp::new();
                                                    use crate::media::MediaBackend;
                                                    yt.get_stream_url(&url).await
                                                }),
                                                video,
                                                is_fav,
                                                in_queue,
                                            ));
                                        }
                                    }
                                    _ => {
                                        // Other queued actions after search are unusual but safe to skip
                                    }
                                }
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        pending_ai_actions.clear();
                        app.set_status(format!("Search failed: {}", e));
                    }
                    Err(e) => {
                        pending_ai_actions.clear();
                        app.set_status(format!("Search failed: {}", e));
                    }
                }
            }
        }

        // Stream URL completion (deferred play)
        if let Some((ref handle, _, _, _)) = pending_stream {
            if handle.is_finished() {
                let (handle, video, is_fav, in_queue) = pending_stream.take().unwrap();
                match handle.await {
                    Ok(Ok(stream)) => {
                        if let Some(mut old) = player.take() {
                            old.stop().await.ok();
                        }
                        let mut p = MpvPlayer::new();
                        if p.play(&stream.audio_url, &video.title).await.is_ok() {
                            let state = crate::player::state::StateFile::new(video.clone(), false);
                            state.write().ok();
                            app.now_playing = Some(tui::app::NowPlaying {
                                video: video.clone(),
                                position_secs: 0,
                                duration_secs: video.duration.map(|d| d as u64).unwrap_or(0),
                                paused: false,
                                volume: 80,
                                speed: 1.0,
                                repeat: crate::player::RepeatMode::Off,
                                shuffle: false,
                                is_fav,
                                in_queue,
                                sleep_deadline: None,
                                eq_preset: "flat".to_string(),
                            });
                            library::history::add_to_history(db, &video, 0).ok();
                            player = Some(p);
                            // Defer resume seek
                            if let Ok(Some(saved_pos)) =
                                library::playback_position::get_position(db, &video.id)
                            {
                                app.pending_resume_seek = Some(saved_pos);
                                if let Some(ref pl) = player {
                                    pl.pause().await.ok();
                                }
                            }
                            // Spawn transcript fetch in background
                            let url_owned = video.url.clone();
                            pending_transcript = Some(tokio::spawn(async move {
                                crate::ai::transcript::fetch_transcript(&url_owned)
                                    .await
                                    .unwrap_or(None)
                            }));
                            pending_transcript_video = Some(video.clone());
                            app.set_status(format!("Playing: {}", video.title));
                        } else {
                            app.set_status(format!("Failed to play: {}", video.title));
                        }
                    }
                    Ok(Err(e)) => app.set_status(format!("Failed to load: {}", e)),
                    Err(e) => app.set_status(format!("Failed to load: {}", e)),
                }
            }
        }

        // Update playback info from mpv (throttled to ~500ms to avoid IPC overload)
        if let Some(ref p) = player {
            if last_mpv_poll.elapsed().as_millis() >= 500 {
                last_mpv_poll = std::time::Instant::now();
                let is_paused = app
                    .now_playing
                    .as_ref()
                    .map(|np| np.paused)
                    .unwrap_or(false);

                // When paused (and no pending resume seek), skip ALL mpv IPC
                // queries — nothing changes while paused and each IPC call adds
                // ~50-100ms of socket overhead, causing noticeable typing lag in
                // the Chat panel. Use fully cached values instead.
                // A lightweight get_paused() check on the slower 2s cadence
                // (state read below) detects if something externally unpaused.
                let (pos, dur, vol, paused) = if is_paused && app.pending_resume_seek.is_none() {
                    let np = app.now_playing.as_ref();
                    (
                        np.map(|n| n.position_secs).unwrap_or(0),
                        np.map(|n| n.duration_secs).unwrap_or(0),
                        np.map(|n| n.volume).unwrap_or(80),
                        true, // known paused
                    )
                } else {
                    let p_val = p.get_position().await.map(|d| d.as_secs()).unwrap_or(0);
                    let d_val = p.get_duration().await.map(|d| d.as_secs()).unwrap_or(0);
                    let v_val = p.get_volume().await.unwrap_or(80);
                    let pa_val = p.get_paused().await.unwrap_or(is_paused);
                    (p_val, d_val, v_val, pa_val)
                };
                app.update_playback(pos, dur, paused, vol);

                // Deferred resume seek: apply once mpv has loaded (dur > 0)
                if let Some(seek_pos) = app.pending_resume_seek {
                    if dur > 0 {
                        p.seek_to(seek_pos as f64).await.ok();
                        if let Some(ref mut np) = app.now_playing {
                            np.position_secs = seek_pos;
                        }
                        app.set_status(format!(
                            "\u{23e9} Resumed from {}:{:02}",
                            seek_pos / 60,
                            seek_pos % 60
                        ));
                        app.pending_resume_seek = None;
                        // Unpause — we paused at play() time to avoid position-0 audio
                        p.resume().await.ok();
                    }
                }

                // Periodic position save (every ~5s)
                if last_position_save.elapsed().as_secs() >= 5 {
                    if let Some(ref np) = app.now_playing {
                        library::playback_position::save_position(db, &np.video.id, pos, dur).ok();
                    }
                    last_position_save = std::time::Instant::now();
                    // Refresh positions cache for list indicators
                    app.saved_positions =
                        library::playback_position::get_all_positions(db).unwrap_or_default();
                }

                // Keep AI context position in sync
                if let Some(ref mut ctx) = ai_context {
                    ctx.current_position = Duration::from_secs(pos);
                }

                // Sync speed/repeat/shuffle/sleep from state file (throttled to ~2s)
                // Note: sleep timer may overshoot by up to ~2s — acceptable for UX
                if last_state_read.elapsed().as_secs() >= 2 {
                    last_state_read = std::time::Instant::now();
                    if let Ok(mut state) = crate::player::state::StateFile::read() {
                        app.update_player_meta(
                            state.speed,
                            state.repeat,
                            state.shuffle,
                            state.sleep_deadline,
                            state
                                .eq_preset
                                .clone()
                                .unwrap_or_else(|| "flat".to_string()),
                        );

                        // ── Sleep timer enforcement ──────────────────────────────
                        if let Some(deadline) = state.sleep_deadline {
                            if chrono::Utc::now() >= deadline {
                                // Time's up — stop playback
                                if let Some(ref mut pl) = player {
                                    pl.stop().await.ok();
                                }
                                player = None;
                                app.now_playing = None;
                                state.sleep_deadline = None;
                                state.write().ok();
                                app.set_status("😴 Sleep timer — playback stopped. Goodnight! 🌙");
                                continue;
                            }
                        }
                    }

                    // When we skipped IPC polling above (paused → full cache),
                    // do a single cheap get_paused() call here (~2s cadence) to
                    // detect external unpause (e.g. AI chat "resume" or remote).
                    if is_paused {
                        if let Ok(actual_paused) = p.get_paused().await {
                            if !actual_paused {
                                if let Some(ref mut np) = app.now_playing {
                                    np.paused = false;
                                }
                            }
                        }
                    }
                }

                // ── Auto-play: detect track end → play next from queue ───────
                // Skip is_finished() when paused — track can't end while paused
                let is_paused_now = app
                    .now_playing
                    .as_ref()
                    .map(|np| np.paused)
                    .unwrap_or(false);
                let is_eof = if is_paused_now {
                    false
                } else {
                    p.is_finished().await.unwrap_or(false)
                };
                let repeat_mode = app
                    .now_playing
                    .as_ref()
                    .map(|np| np.repeat)
                    .unwrap_or(crate::player::RepeatMode::Off);

                // RepeatOne is handled by mpv loop-file, skip auto-play
                if is_eof && repeat_mode != crate::player::RepeatMode::One {
                    // Clear saved position for the finished track
                    if let Some(ref np) = app.now_playing {
                        library::playback_position::clear_position(db, &np.video.id).ok();
                    }
                    // RepeatAll: re-add current track to end of queue before popping next
                    if repeat_mode == crate::player::RepeatMode::All {
                        if let Some(ref np) = app.now_playing {
                            library::queue::add_to_queue(db, &np.video).ok();
                        }
                    }

                    // Shuffle: pick random from queue
                    let next_entry = if app
                        .now_playing
                        .as_ref()
                        .map(|np| np.shuffle)
                        .unwrap_or(false)
                    {
                        let q = library::queue::get_queue(db).unwrap_or_default();
                        if q.is_empty() {
                            None
                        } else {
                            use std::collections::hash_map::DefaultHasher;
                            use std::hash::{Hash, Hasher};
                            let mut hasher = DefaultHasher::new();
                            std::time::SystemTime::now().hash(&mut hasher);
                            let idx = (hasher.finish() as usize) % q.len();
                            let entry = q[idx].clone();
                            library::queue::remove_from_queue(db, entry.id).ok();
                            Some(entry)
                        }
                    } else {
                        library::queue::pop_next(db).unwrap_or(None)
                    };

                    if let Some(entry) = next_entry {
                        app.set_status(format!("⏭ Next: {}...", entry.title));
                        let is_fav =
                            library::favorites::is_favorite(db, &entry.video_id).unwrap_or(false);
                        let in_queue =
                            library::queue::is_in_queue(db, &entry.video_id).unwrap_or(false);
                        let video = crate::media::MediaInfo {
                            id: entry.video_id.clone(),
                            title: entry.title.clone(),
                            channel: entry.channel.clone(),
                            url: entry.url.clone(),
                            duration: entry.duration_secs.map(|d| d as f64),
                            view_count: None,
                            thumbnail: None,
                            description: None,
                            source: crate::media::Source::default(),
                            extractor_key: None,
                        };
                        let url = entry.url.clone();
                        pending_stream = Some((
                            tokio::spawn(async move {
                                let yt = YtDlp::new();
                                use crate::media::MediaBackend;
                                yt.get_stream_url(&url).await
                            }),
                            video,
                            is_fav,
                            in_queue,
                        ));
                    } else if repeat_mode != crate::player::RepeatMode::All {
                        // Queue empty, not repeat-all → stop
                        app.set_status("⏹ Queue finished");
                    }
                }
            } // end throttle check
        }

        // Handle events
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(KeyEvent {
                code, modifiers, ..
            }) = event::read()?
            {
                // Universal: Tab switches panels
                if code == KeyCode::Tab {
                    let next = match app.panel {
                        Panel::Search => Panel::Results,
                        Panel::Results => Panel::Lyrics,
                        Panel::Lyrics => Panel::Queue,
                        Panel::Queue => Panel::Favorites,
                        Panel::Favorites => Panel::History,
                        Panel::History => Panel::Playlists,
                        Panel::Playlists => Panel::Chat,
                        Panel::Chat => Panel::Help,
                        Panel::Help => Panel::Search,
                    };
                    // Preload data for panels that need it
                    match next {
                        Panel::Queue => {
                            app.queue_items = library::queue::get_queue(db).unwrap_or_default();
                        }
                        Panel::Favorites => {
                            app.fav_items =
                                library::favorites::get_favorites(db).unwrap_or_default();
                        }
                        Panel::History => {
                            app.history_items =
                                library::history::get_history(db, 50).unwrap_or_default();
                        }
                        Panel::Playlists => {
                            app.playlist_list =
                                library::playlist::list_playlists(db).unwrap_or_default();
                            app.playlist_items_view = None;
                        }
                        _ => {}
                    }
                    app.set_panel(next);
                    continue;
                }

                let mut handled = false;

                // ── Playlist picker modal (takes priority over everything) ────
                if app.playlist_picker.is_some() {
                    match code {
                        KeyCode::Esc => {
                            app.close_playlist_picker();
                            app.set_status("Cancelled");
                        }
                        KeyCode::Up => {
                            app.picker_prev();
                        }
                        KeyCode::Down => {
                            app.picker_next();
                        }
                        KeyCode::Enter => {
                            // Clone needed data out of the picker before mutating
                            let (video, playlist_name) = {
                                let pk = app.playlist_picker.as_ref().unwrap();
                                if let Some(pl) = pk.playlists.get(pk.selected) {
                                    (pk.video.clone(), Some(pl.name.clone()))
                                } else {
                                    (pk.video.clone(), None)
                                }
                            };
                            if let Some(name) = playlist_name {
                                match library::playlist::add_to_playlist(db, &name, &video) {
                                    Ok(true) => {
                                        app.set_status(format!("✅ Added to '{}'", name));
                                    }
                                    Ok(false) => {
                                        app.set_status(format!("⚠ Already in '{}'", name));
                                    }
                                    Err(e) => {
                                        app.set_status(format!("Error: {}", e));
                                    }
                                }
                            }
                            app.close_playlist_picker();
                        }
                        _ => {}
                    }
                    continue; // Skip all other input handling
                }

                match app.panel {
                    Panel::Search => {
                        // Ctrl+S: cycle search source before other key handling
                        if code == KeyCode::Char('s')
                            && modifiers.contains(crossterm::event::KeyModifiers::CONTROL)
                        {
                            app.cycle_search_source();
                            continue;
                        }
                        match code {
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
                                    app.set_status("🔍 Searching...");

                                    // Spawn search in background to keep TUI responsive
                                    let fetch_count = app.search_page_size * 5;
                                    let q = query.clone();
                                    let source = app.search_source.clone();
                                    pending_search = Some((
                                        tokio::spawn(async move {
                                            let yt = YtDlp::new();
                                            use crate::media::MediaBackend;
                                            yt.search(&q, fetch_count, &source).await
                                        }),
                                        query,
                                    ));
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
                        } // match code
                    } // Panel::Search

                    Panel::Results => {
                        handled = true;
                        match code {
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
                                if let Some(video) =
                                    app.search_results.get(app.selected_index).cloned()
                                {
                                    // Save position of current track before switching
                                    if let Some(ref np) = app.now_playing {
                                        library::playback_position::save_position(
                                            db,
                                            &np.video.id,
                                            np.position_secs,
                                            np.duration_secs,
                                        )
                                        .ok();
                                    }
                                    app.set_status(format!("⏳ Loading: {}...", video.title));
                                    let is_fav = library::favorites::is_favorite(db, &video.id)
                                        .unwrap_or(false);
                                    let in_queue =
                                        library::queue::is_in_queue(db, &video.id).unwrap_or(false);
                                    let url = video.url.clone();
                                    pending_stream = Some((
                                        tokio::spawn(async move {
                                            let yt = YtDlp::new();
                                            use crate::media::MediaBackend;
                                            yt.get_stream_url(&url).await
                                        }),
                                        video,
                                        is_fav,
                                        in_queue,
                                    ));
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
                                            app.set_status(
                                                "⚠ Already in queue — skipped".to_string(),
                                            );
                                        }
                                        Err(e) => app.set_status(format!("Queue error: {}", e)),
                                    }
                                }
                            }
                            KeyCode::Char('f') => {
                                if let Some(video) = app.search_results.get(app.selected_index) {
                                    let vid = video.id.clone();
                                    let vtitle = video.title.clone();
                                    let is_fav =
                                        library::favorites::is_favorite(db, &vid).unwrap_or(false);
                                    if is_fav {
                                        library::favorites::remove_favorite(db, &vid).ok();
                                        app.set_status(format!("💔 Removed: {}", vtitle));
                                    } else {
                                        library::favorites::add_favorite(db, video).ok();
                                        app.set_status(format!("❤️ Favorited: {}", vtitle));
                                    }
                                    // Also sync NowPlaying if same video
                                    if let Some(ref mut np) = app.now_playing {
                                        if np.video.id == vid {
                                            np.is_fav = !is_fav;
                                        }
                                    }
                                }
                            }
                            KeyCode::Char('l') => {
                                if let Some(video) =
                                    app.search_results.get(app.selected_index).cloned()
                                {
                                    let playlists =
                                        library::playlist::list_playlists(db).unwrap_or_default();
                                    if playlists.is_empty() {
                                        app.set_status(
                                            "No playlists. Go to Playlists tab → [n] to create one",
                                        );
                                    } else {
                                        app.open_playlist_picker(video, playlists);
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
                            _ => {
                                handled = false;
                            }
                        }
                    }

                    Panel::Lyrics => {
                        match (code, modifiers) {
                            (KeyCode::Esc, _) | (KeyCode::Char('q'), _) => {
                                app.set_panel(Panel::Search);
                            }
                            // Shift+Up: scroll up (manual mode)
                            (KeyCode::Up, KeyModifiers::SHIFT) => {
                                app.lyrics_auto_scroll = false;
                                app.lyrics_scroll = app.lyrics_scroll.saturating_sub(1);
                            }
                            // Shift+Down: scroll down (manual mode)
                            (KeyCode::Down, KeyModifiers::SHIFT) => {
                                app.lyrics_auto_scroll = false;
                                app.lyrics_scroll = app.lyrics_scroll.saturating_add(1);
                            }
                            // 0: reset to auto-scroll
                            (KeyCode::Char('0'), _) => {
                                app.lyrics_auto_scroll = true;
                                app.lyrics_scroll = 0;
                            }
                            _ => {}
                        }
                        handled = true;
                    }

                    Panel::Queue => {
                        handled = true;
                        match code {
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
                                // Play selected queue item
                                if let Some(entry) =
                                    app.queue_items.get(app.selected_index).cloned()
                                {
                                    if let Some(ref np) = app.now_playing {
                                        library::playback_position::save_position(
                                            db,
                                            &np.video.id,
                                            np.position_secs,
                                            np.duration_secs,
                                        )
                                        .ok();
                                    }
                                    app.set_status(format!("⏳ Loading: {}...", entry.title));
                                    let video = crate::media::MediaInfo {
                                        id: entry.video_id.clone(),
                                        title: entry.title.clone(),
                                        channel: entry.channel.clone(),
                                        duration: entry.duration_secs.map(|s| s as f64),
                                        view_count: None,
                                        thumbnail: None,
                                        url: entry.url.clone(),
                                        description: None,
                                        source: crate::media::Source::default(),
                                        extractor_key: None,
                                    };
                                    let url = entry.url.clone();
                                    pending_stream = Some((
                                        tokio::spawn(async move {
                                            let yt = YtDlp::new();
                                            use crate::media::MediaBackend;
                                            yt.get_stream_url(&url).await
                                        }),
                                        video,
                                        false,
                                        true, // in_queue
                                    ));
                                }
                            }
                            KeyCode::Char('d') => {
                                // Copy id out first to release the immutable borrow on app.queue_items
                                let entry_id =
                                    app.queue_items.get(app.selected_index).map(|e| e.id);
                                if let Some(id) = entry_id {
                                    match library::queue::remove_from_queue(db, id) {
                                        Ok(true) => {
                                            // Reload queue and clamp index
                                            app.queue_items =
                                                library::queue::get_queue(db).unwrap_or_default();
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
                            KeyCode::Char('l') => {
                                if let Some(entry) =
                                    app.queue_items.get(app.selected_index).cloned()
                                {
                                    let video = crate::media::MediaInfo {
                                        id: entry.video_id,
                                        title: entry.title,
                                        channel: entry.channel,
                                        url: entry.url,
                                        duration: entry.duration_secs.map(|d| d as f64),
                                        view_count: None,
                                        thumbnail: None,
                                        description: None,
                                        source: crate::media::Source::default(),
                                        extractor_key: None,
                                    };
                                    let playlists =
                                        library::playlist::list_playlists(db).unwrap_or_default();
                                    if playlists.is_empty() {
                                        app.set_status(
                                            "No playlists. Go to Playlists tab → [n] to create one",
                                        );
                                    } else {
                                        app.open_playlist_picker(video, playlists);
                                    }
                                }
                            }
                            _ => {
                                handled = false;
                            }
                        }
                    }

                    Panel::Favorites => {
                        handled = true;
                        match code {
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
                                if let Some(entry) = app.fav_items.get(app.selected_index).cloned()
                                {
                                    if let Some(ref np) = app.now_playing {
                                        library::playback_position::save_position(
                                            db,
                                            &np.video.id,
                                            np.position_secs,
                                            np.duration_secs,
                                        )
                                        .ok();
                                    }
                                    app.set_status(format!("⏳ Loading: {}...", entry.title));
                                    let in_queue = library::queue::is_in_queue(db, &entry.video_id)
                                        .unwrap_or(false);
                                    let video = crate::media::MediaInfo {
                                        id: entry.video_id.clone(),
                                        title: entry.title.clone(),
                                        channel: entry.channel.clone(),
                                        duration: entry.duration_secs.map(|s| s as f64),
                                        view_count: None,
                                        thumbnail: None,
                                        url: entry.url.clone(),
                                        description: None,
                                        source: crate::media::Source::default(),
                                        extractor_key: None,
                                    };
                                    let url = entry.url.clone();
                                    pending_stream = Some((
                                        tokio::spawn(async move {
                                            let yt = YtDlp::new();
                                            use crate::media::MediaBackend;
                                            yt.get_stream_url(&url).await
                                        }),
                                        video,
                                        true, // is_fav
                                        in_queue,
                                    ));
                                }
                            }
                            KeyCode::Char('d') => {
                                let vid = app
                                    .fav_items
                                    .get(app.selected_index)
                                    .map(|e| e.video_id.clone());
                                if let Some(video_id) = vid {
                                    match library::favorites::remove_favorite(db, &video_id) {
                                        Ok(true) => {
                                            app.fav_items = library::favorites::get_favorites(db)
                                                .unwrap_or_default();
                                            if app.selected_index >= app.fav_items.len()
                                                && !app.fav_items.is_empty()
                                            {
                                                app.selected_index = app.fav_items.len() - 1;
                                            } else if app.fav_items.is_empty() {
                                                app.selected_index = 0;
                                            }
                                            app.set_status("Removed from favorites 💔");
                                            // Sync player if same video
                                            if let Some(ref mut np) = app.now_playing {
                                                if np.video.id == video_id {
                                                    np.is_fav = false;
                                                }
                                            }
                                        }
                                        Ok(false) => {
                                            app.set_status("Not found in favorites");
                                        }
                                        Err(e) => {
                                            app.set_status(format!("Remove failed: {}", e));
                                        }
                                    }
                                }
                            }
                            KeyCode::Char('l') => {
                                if let Some(entry) = app.fav_items.get(app.selected_index).cloned()
                                {
                                    let video = crate::media::MediaInfo {
                                        id: entry.video_id,
                                        title: entry.title,
                                        channel: entry.channel,
                                        url: entry.url,
                                        duration: entry.duration_secs.map(|d| d as f64),
                                        view_count: None,
                                        thumbnail: None,
                                        description: None,
                                        source: crate::media::Source::default(),
                                        extractor_key: None,
                                    };
                                    let playlists =
                                        library::playlist::list_playlists(db).unwrap_or_default();
                                    if playlists.is_empty() {
                                        app.set_status(
                                            "No playlists. Go to Playlists tab → [n] to create one",
                                        );
                                    } else {
                                        app.open_playlist_picker(video, playlists);
                                    }
                                }
                            }
                            _ => {
                                handled = false;
                            }
                        }
                    }

                    Panel::History => {
                        handled = true;
                        match code {
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
                                if let Some(entry) =
                                    app.history_items.get(app.selected_index).cloned()
                                {
                                    if let Some(ref np) = app.now_playing {
                                        library::playback_position::save_position(
                                            db,
                                            &np.video.id,
                                            np.position_secs,
                                            np.duration_secs,
                                        )
                                        .ok();
                                    }
                                    app.set_status(format!("⏳ Loading: {}...", entry.title));
                                    let is_fav =
                                        library::favorites::is_favorite(db, &entry.video_id)
                                            .unwrap_or(false);
                                    let in_queue = library::queue::is_in_queue(db, &entry.video_id)
                                        .unwrap_or(false);
                                    let video = crate::media::MediaInfo {
                                        id: entry.video_id.clone(),
                                        title: entry.title.clone(),
                                        channel: entry.channel.clone(),
                                        duration: entry.duration_secs.map(|s| s as f64),
                                        view_count: None,
                                        thumbnail: None,
                                        url: entry.url.clone(),
                                        description: None,
                                        source: crate::media::Source::default(),
                                        extractor_key: None,
                                    };
                                    let url = entry.url.clone();
                                    pending_stream = Some((
                                        tokio::spawn(async move {
                                            let yt = YtDlp::new();
                                            use crate::media::MediaBackend;
                                            yt.get_stream_url(&url).await
                                        }),
                                        video,
                                        is_fav,
                                        in_queue,
                                    ));
                                }
                            }
                            KeyCode::Char('l') => {
                                if let Some(entry) =
                                    app.history_items.get(app.selected_index).cloned()
                                {
                                    let video = crate::media::MediaInfo {
                                        id: entry.video_id,
                                        title: entry.title,
                                        channel: entry.channel,
                                        url: entry.url,
                                        duration: entry.duration_secs.map(|d| d as f64),
                                        view_count: None,
                                        thumbnail: None,
                                        description: None,
                                        source: crate::media::Source::default(),
                                        extractor_key: None,
                                    };
                                    let playlists =
                                        library::playlist::list_playlists(db).unwrap_or_default();
                                    if playlists.is_empty() {
                                        app.set_status(
                                            "No playlists. Go to Playlists tab → [n] to create one",
                                        );
                                    } else {
                                        app.open_playlist_picker(video, playlists);
                                    }
                                }
                            }
                            _ => {
                                handled = false;
                            }
                        }
                    }

                    Panel::Playlists => {
                        handled = true;
                        match code {
                            // ── Input mode: typing a new playlist name ──
                            _ if app.playlist_name_input.is_some() => match code {
                                KeyCode::Esc => {
                                    app.playlist_name_input = None;
                                    app.set_status("Cancelled");
                                }
                                KeyCode::Enter => {
                                    let name = app.playlist_name_input.take().unwrap_or_default();
                                    let name = name.trim().to_string();
                                    if !name.is_empty() {
                                        match library::playlist::create_playlist(db, &name) {
                                            Ok(_) => {
                                                app.playlist_list =
                                                    library::playlist::list_playlists(db)
                                                        .unwrap_or_default();
                                                app.set_status(format!(
                                                    "✅ Created playlist: {}",
                                                    name
                                                ));
                                            }
                                            Err(e) => app.set_status(format!("Error: {}", e)),
                                        }
                                    } else {
                                        app.set_status("Playlist name cannot be empty");
                                    }
                                }
                                KeyCode::Backspace => {
                                    if let Some(ref mut s) = app.playlist_name_input {
                                        s.pop();
                                    }
                                }
                                KeyCode::Char(c) => {
                                    if let Some(ref mut s) = app.playlist_name_input {
                                        s.push(c);
                                    }
                                }
                                _ => {}
                            },
                            KeyCode::Esc | KeyCode::Char('q') => {
                                if app.playlist_items_view.is_some() {
                                    // Go back to playlist list
                                    app.playlist_items_view = None;
                                    app.playlist_list =
                                        library::playlist::list_playlists(db).unwrap_or_default();
                                    app.selected_index = 0;
                                } else {
                                    app.set_panel(Panel::Search);
                                }
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                app.select_prev();
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                app.select_next();
                            }
                            KeyCode::Enter => {
                                if app.playlist_items_view.is_some() {
                                    // Play selected item from playlist items view
                                    // Clone out the items + selected index before mutating
                                    let play_info =
                                        app.playlist_items_view.as_ref().and_then(|(_, items)| {
                                            items.get(app.selected_index).cloned().map(|item| {
                                                let remaining: Vec<_> = items
                                                    .iter()
                                                    .skip(app.selected_index + 1)
                                                    .cloned()
                                                    .collect();
                                                (item, remaining)
                                            })
                                        });
                                    if let Some((item, remaining_items)) = play_info {
                                        if let Some(ref np) = app.now_playing {
                                            library::playback_position::save_position(
                                                db,
                                                &np.video.id,
                                                np.position_secs,
                                                np.duration_secs,
                                            )
                                            .ok();
                                        }
                                        app.set_status(format!("⏳ Loading: {}...", item.title));

                                        // Queue remaining playlist items (fast DB I/O)
                                        library::queue::clear_queue(db).ok();
                                        let mut queued = 0usize;
                                        for ri in &remaining_items {
                                            let rv = crate::media::MediaInfo {
                                                id: ri.video_id.clone(),
                                                title: ri.title.clone(),
                                                channel: ri.channel.clone(),
                                                url: ri.url.clone(),
                                                duration: ri.duration_secs.map(|d| d as f64),
                                                view_count: None,
                                                thumbnail: None,
                                                description: None,
                                                source: crate::media::Source::default(),
                                                extractor_key: None,
                                            };
                                            if library::queue::add_to_queue(db, &rv)
                                                .unwrap_or(false)
                                            {
                                                queued += 1;
                                            }
                                        }
                                        app.queue_items =
                                            library::queue::get_queue(db).unwrap_or_default();

                                        let video = crate::media::MediaInfo {
                                            id: item.video_id.clone(),
                                            title: item.title.clone(),
                                            channel: item.channel.clone(),
                                            url: item.url.clone(),
                                            duration: item.duration_secs.map(|d| d as f64),
                                            view_count: None,
                                            thumbnail: None,
                                            description: None,
                                            source: crate::media::Source::default(),
                                            extractor_key: None,
                                        };
                                        let url = item.url.clone();
                                        pending_stream = Some((
                                            tokio::spawn(async move {
                                                let yt = YtDlp::new();
                                                use crate::media::MediaBackend;
                                                yt.get_stream_url(&url).await
                                            }),
                                            video,
                                            false,
                                            false,
                                        ));
                                        if queued > 0 {
                                            app.set_status(format!(
                                                "⏳ Loading: {} · {} more queued",
                                                item.title, queued
                                            ));
                                        }
                                    }
                                } else {
                                    // Enter playlist detail view
                                    if let Some(pl) = app.playlist_list.get(app.selected_index) {
                                        let name = pl.name.clone();
                                        match library::playlist::get_playlist_items(db, &name) {
                                            Ok(items) => {
                                                app.playlist_items_view = Some((name, items));
                                                app.selected_index = 0;
                                            }
                                            Err(e) => app.set_status(format!("Error: {}", e)),
                                        }
                                    }
                                }
                            }
                            // Play entire playlist (load to queue)
                            KeyCode::Char('p') if app.playlist_items_view.is_none() => {
                                if let Some(pl) = app.playlist_list.get(app.selected_index) {
                                    library::queue::clear_queue(db).ok();
                                    match library::playlist::load_playlist_to_queue(db, &pl.name) {
                                        Ok(count) => {
                                            app.queue_items =
                                                library::queue::get_queue(db).unwrap_or_default();
                                            app.set_status(format!(
                                                "🎶 Loaded {} tracks from '{}' into queue",
                                                count, pl.name
                                            ));
                                            // Auto-play first
                                            if let Some(entry) =
                                                library::queue::pop_next(db).unwrap_or(None)
                                            {
                                                app.set_status(format!(
                                                    "⏳ Loading: {}...",
                                                    entry.title
                                                ));
                                                let video = crate::media::MediaInfo {
                                                    id: entry.video_id.clone(),
                                                    title: entry.title.clone(),
                                                    channel: entry.channel.clone(),
                                                    url: entry.url.clone(),
                                                    duration: entry.duration_secs.map(|d| d as f64),
                                                    view_count: None,
                                                    thumbnail: None,
                                                    description: None,
                                                    source: crate::media::Source::default(),
                                                    extractor_key: None,
                                                };
                                                let url = entry.url.clone();
                                                pending_stream = Some((
                                                    tokio::spawn(async move {
                                                        let yt = YtDlp::new();
                                                        use crate::media::MediaBackend;
                                                        yt.get_stream_url(&url).await
                                                    }),
                                                    video,
                                                    false,
                                                    false,
                                                ));
                                            }
                                        }
                                        Err(e) => app.set_status(format!("Error: {}", e)),
                                    }
                                }
                            }
                            // New playlist: n (only in list view)
                            KeyCode::Char('n') if app.playlist_items_view.is_none() => {
                                app.playlist_name_input = Some(String::new());
                                app.set_status("Enter a name for the new playlist");
                            }
                            // Delete item from playlist (detail view)
                            KeyCode::Char('d') if app.playlist_items_view.is_some() => {
                                // Clone out the info we need
                                let info =
                                    app.playlist_items_view.as_ref().and_then(|(name, items)| {
                                        items.get(app.selected_index).map(|item| {
                                            (
                                                name.clone(),
                                                item.video_id.clone(),
                                                item.title.clone(),
                                            )
                                        })
                                    });
                                if let Some((pl_name, vid, title)) = info {
                                    match library::playlist::remove_from_playlist(
                                        db, &pl_name, &vid,
                                    ) {
                                        Ok(true) => {
                                            // Reload items
                                            let items =
                                                library::playlist::get_playlist_items(db, &pl_name)
                                                    .unwrap_or_default();
                                            if app.selected_index >= items.len()
                                                && !items.is_empty()
                                            {
                                                app.selected_index = items.len() - 1;
                                            } else if items.is_empty() {
                                                app.selected_index = 0;
                                            }
                                            app.playlist_items_view = Some((pl_name, items));
                                            app.set_status(format!("🗑 Removed: {}", title));
                                        }
                                        Ok(false) => app.set_status("Item not found"),
                                        Err(e) => app.set_status(format!("Error: {}", e)),
                                    }
                                }
                            }
                            // Delete playlist: d (only in list view)
                            KeyCode::Char('d') if app.playlist_items_view.is_none() => {
                                if let Some(pl) = app.playlist_list.get(app.selected_index) {
                                    let name = pl.name.clone();
                                    match library::playlist::delete_playlist(db, &name) {
                                        Ok(_) => {
                                            app.playlist_list =
                                                library::playlist::list_playlists(db)
                                                    .unwrap_or_default();
                                            if app.selected_index > 0
                                                && app.selected_index >= app.playlist_list.len()
                                            {
                                                app.selected_index =
                                                    app.playlist_list.len().saturating_sub(1);
                                            }
                                            app.set_status(format!("🗑 Deleted playlist: {}", name));
                                        }
                                        Err(e) => app.set_status(format!("Error: {}", e)),
                                    }
                                }
                            }
                            _ => {
                                handled = false;
                            }
                        }
                    }

                    Panel::Chat => match code {
                        KeyCode::Esc => {
                            app.set_panel(Panel::Search);
                        }
                        KeyCode::Enter => {
                            let input = app.chat_input.trim().to_string();
                            if !input.is_empty() {
                                app.chat_input.clear();
                                app.push_chat_message("user", &input);

                                if ai_context.is_some() {
                                    if let Some(ref ai_cfg) = config.ai {
                                        match ai_cfg.resolve(None) {
                                            Ok(resolved) => {
                                                app.chat_loading = true;
                                                app.set_status("AI is thinking...");
                                                // Move context into spawned task;
                                                // it will be returned on completion.
                                                let mut ctx = ai_context.take().unwrap();
                                                // —— Sync position (cheap, needed for lyric context) ——
                                                if let Some(ref np) = app.now_playing {
                                                    ctx.current_position =
                                                        std::time::Duration::from_secs(np.position_secs);
                                                }
                                                // —— Sync search results if still empty ——
                                                if ctx.search_results.is_empty() && !app.search_results.is_empty() {
                                                    ctx.search_results = app
                                                        .search_results
                                                        .iter()
                                                        .map(|v| v.title.clone())
                                                        .collect();
                                                }
                                                // NOTE: transcript is NOT injected here — it is set
                                                // once when pending_transcript completes (or on
                                                // chat-task return if it arrived while in-flight).
                                                pending_chat = Some(tokio::spawn(async move {
                                                    let result = ai_chat(&mut ctx, &input, &resolved).await;
                                                    (ctx, result)
                                                }));
                                            }
                                            Err(e) => {
                                                app.push_chat_message(
                                                    "assistant",
                                                    &format!("Config error: {}", e),
                                                );
                                            }
                                        }
                                    } else {
                                        app.push_chat_message(
                                            "assistant",
                                            "AI not configured. Run: aux config ai --setup",
                                        );
                                    }
                                } else if pending_chat.is_some() {
                                    app.push_chat_message(
                                        "assistant",
                                        "Still thinking about the previous message...",
                                    );
                                } else {
                                    // No video playing — create a lightweight context
                                    // so the user can still search, pick results, and chat.
                                    let placeholder = crate::media::MediaInfo {
                                        id: String::new(),
                                        title: "(no track playing)".to_string(),
                                        channel: None,
                                        url: String::new(),
                                        duration: None,
                                        view_count: None,
                                        thumbnail: None,
                                        description: None,
                                        source: crate::media::Source::default(),
                                        extractor_key: None,
                                    };
                                    let mut ctx = VideoContext::new(placeholder, None);
                                    // Carry over search results if any
                                    ctx.search_results = app
                                        .search_results
                                        .iter()
                                        .map(|v| format!("{}", v))
                                        .collect();
                                    ai_context = Some(ctx);
                                    // Now retry — ai_context is Some, will enter the branch above
                                    let mut ctx = ai_context.take().unwrap();
                                    if let Some(ref ai_cfg) = config.ai {
                                        match ai_cfg.resolve(None) {
                                            Ok(resolved) => {
                                                app.chat_loading = true;
                                                app.set_status("AI is thinking...");
                                                pending_chat = Some(tokio::spawn(async move {
                                                    let result = ai_chat(&mut ctx, &input, &resolved).await;
                                                    (ctx, result)
                                                }));
                                            }
                                            Err(e) => {
                                                ai_context = Some(ctx);
                                                app.push_chat_message(
                                                    "assistant",
                                                    &format!("Config error: {}", e),
                                                );
                                            }
                                        }
                                    } else {
                                        ai_context = Some(ctx);
                                        app.push_chat_message(
                                            "assistant",
                                            "AI not configured. Run: aux config ai --setup",
                                        );
                                    }
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
                        handled = true;
                        match code {
                            KeyCode::Esc | KeyCode::Char('q') => {
                                app.help_scroll = 0;
                                app.set_panel(Panel::Search);
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                app.help_scroll = app.help_scroll.saturating_sub(1);
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                app.help_scroll = app.help_scroll.saturating_add(1);
                            }
                            _ => {}
                        }
                    }
                }

                // Player controls available from any panel when playing
                // (skip if the panel already consumed this key, e.g. typing in Search)
                if !handled && player.is_some() {
                    match (code, modifiers) {
                        // Pause / resume (restart if track has ended)
                        (KeyCode::Char(' '), _) => {
                            if let Some(ref p) = player {
                                // If the track has finished, restart from the beginning
                                let finished = p.is_finished().await.unwrap_or(false);
                                if finished {
                                    p.seek_to(0.0).await.ok();
                                    p.resume().await.ok();
                                    if let Some(ref mut np) = app.now_playing {
                                        np.paused = false;
                                        np.position_secs = 0;
                                    }
                                    app.set_status("▶ Restarted");
                                } else {
                                    p.toggle_pause().await.ok();
                                    // Read actual state from mpv to stay in sync
                                    if let Some(ref mut np) = app.now_playing {
                                        match p.get_paused().await {
                                            Ok(paused) => np.paused = paused,
                                            Err(_) => np.paused = !np.paused, // fallback
                                        }
                                    }
                                }
                            }
                        }
                        // Seek ±10s (non-list panels) or Shift+←/→ everywhere
                        (KeyCode::Left, KeyModifiers::NONE)
                            if !matches!(
                                app.panel,
                                Panel::Results | Panel::Queue | Panel::History
                            ) =>
                        {
                            if let Some(ref p) = player {
                                p.seek(-10.0).await.ok();
                            }
                        }
                        (KeyCode::Right, KeyModifiers::NONE)
                            if !matches!(
                                app.panel,
                                Panel::Results | Panel::Queue | Panel::History
                            ) =>
                        {
                            if let Some(ref p) = player {
                                p.seek(10.0).await.ok();
                            }
                        }
                        (KeyCode::Left, KeyModifiers::SHIFT) => {
                            if let Some(ref p) = player {
                                p.seek(-60.0).await.ok();
                            }
                        }
                        (KeyCode::Right, KeyModifiers::SHIFT) => {
                            if let Some(ref p) = player {
                                p.seek(60.0).await.ok();
                            }
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
                            // Save position of current track before skipping
                            if let Some(ref np) = app.now_playing {
                                library::playback_position::save_position(
                                    db,
                                    &np.video.id,
                                    np.position_secs,
                                    np.duration_secs,
                                )
                                .ok();
                            }
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
                            // Save position before stopping
                            if let Some(ref np) = app.now_playing {
                                library::playback_position::save_position(
                                    db,
                                    &np.video.id,
                                    np.position_secs,
                                    np.duration_secs,
                                )
                                .ok();
                            }
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
                                // Tell mpv to loop (RepeatOne = loop-file inf)
                                if let Some(ref p) = player {
                                    p.set_loop_file(state.repeat == crate::player::RepeatMode::One)
                                        .await
                                        .ok();
                                }
                                app.set_status(format!("Repeat: {}", label));
                            }
                        }
                        // Shuffle: z
                        (KeyCode::Char('z'), _) => {
                            if let Ok(mut state) = crate::player::state::StateFile::read() {
                                state.shuffle = !state.shuffle;
                                let on = state.shuffle;
                                state.write().ok();
                                app.set_status(format!(
                                    "Shuffle: {}",
                                    if on { "on 🔀" } else { "off" }
                                ));
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
                                // Reload favorites list if visible
                                if app.panel == Panel::Favorites {
                                    app.fav_items =
                                        library::favorites::get_favorites(db).unwrap_or_default();
                                }
                            }
                        }
                        // Toggle queue: a
                        (KeyCode::Char('a'), _) => {
                            if let Some(ref mut np) = app.now_playing {
                                if np.in_queue {
                                    library::queue::remove_from_queue_by_video_id(db, &np.video.id)
                                        .ok();
                                    np.in_queue = false;
                                    // Reload queue panel if visible
                                    app.queue_items =
                                        library::queue::get_queue(db).unwrap_or_default();
                                    app.set_status("📋✗ Removed from queue");
                                } else {
                                    match library::queue::add_to_queue(db, &np.video) {
                                        Ok(_) => {
                                            np.in_queue = true;
                                            let len = library::queue::queue_length(db).unwrap_or(0);
                                            // Reload queue panel if visible
                                            app.queue_items =
                                                library::queue::get_queue(db).unwrap_or_default();
                                            app.set_status(format!("📋 Added to queue (#{})", len));
                                        }
                                        Err(e) => app.set_status(format!("Queue error: {}", e)),
                                    }
                                }
                            }
                        }
                        // Sleep timer: t (cycle 15m → 30m → 1h → 2h → off)
                        (KeyCode::Char('t'), _) => {
                            use chrono::Utc;
                            if let Ok(mut state) = crate::player::state::StateFile::read() {
                                let now = Utc::now();
                                // Determine next step based on current remaining time
                                let remaining_mins = state
                                    .sleep_deadline
                                    .map(|d| (d - now).num_minutes().max(0))
                                    .unwrap_or(0);
                                let (next_mins, label) =
                                    if remaining_mins == 0 || state.sleep_deadline.is_none() {
                                        (15, "15min")
                                    } else if remaining_mins <= 15 {
                                        (30, "30min")
                                    } else if remaining_mins <= 30 {
                                        (60, "1h")
                                    } else if remaining_mins <= 60 {
                                        (120, "2h")
                                    } else {
                                        (0, "off") // cancel
                                    };

                                if next_mins == 0 {
                                    state.sleep_deadline = None;
                                    state.write().ok();
                                    app.set_status("😴 Sleep timer cancelled");
                                } else {
                                    let deadline = now + chrono::Duration::minutes(next_mins);
                                    state.sleep_deadline = Some(deadline);
                                    state.write().ok();
                                    app.set_status(format!(
                                        "😴 Sleep in {} ({})",
                                        label,
                                        deadline.format("%H:%M")
                                    ));
                                }
                            }
                        }
                        // Chat: c (hint — terminal only)
                        (KeyCode::Char('c'), _) => {
                            app.set_status("💬 Chat: quit TUI then run: aux chat");
                        }
                        // Equalizer cycle: e
                        (KeyCode::Char('e'), _) => {
                            let presets = eq_preset_names();
                            let current = crate::player::state::StateFile::read()
                                .ok()
                                .and_then(|s| s.eq_preset)
                                .unwrap_or_else(|| "flat".to_string());
                            let idx = presets.iter().position(|p| *p == current).unwrap_or(0);
                            let next = presets[(idx + 1) % presets.len()];
                            if let Ok(mut sf) = crate::player::state::StateFile::read() {
                                sf.eq_preset = Some(next.to_string());
                                sf.write().ok();
                            }
                            if let Some(ref p) = player {
                                let filter = eq_preset_filter(next).unwrap_or("");
                                p.set_audio_filter(filter).await.ok();
                            }
                            app.set_status(format!("🎛️ EQ: {}", next));
                        }
                        _ => {}
                    }
                }
            }
        }

        // Sleep timer check removed — already handled in the mpv poll block above

        if app.should_quit {
            break;
        }
    }

    // Save position before cleanup
    if let Some(ref np) = app.now_playing {
        library::playback_position::save_position(
            db,
            &np.video.id,
            np.position_secs,
            np.duration_secs,
        )
        .ok();
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
        Some(ConfigAction::Player {
            action:
                Some(PlayerAction::Set {
                    volume,
                    search_results,
                    backend,
                }),
        }) => {
            cc::player_set(config, volume, search_results, backend)?;
        }
        Some(ConfigAction::Media { action: None }) => {
            cc::show_media(config);
        }
        Some(ConfigAction::Media {
            action:
                Some(MediaAction::Set {
                    format,
                    backend,
                    default_source,
                }),
        }) => {
            cc::media_set(config, format, backend, default_source)?;
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
