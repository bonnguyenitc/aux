mod ai;
mod cli;
mod config;
mod config_cmd;
mod error;
mod interactive;
mod library;
mod player;
mod tui;
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
        Commands::Play { url } => {
            cmd_play(&url, &config, &db).await?;
        }
        Commands::Now => {
            println!(
                "  {}",
                "(playback status not yet implemented for detached mode)".dimmed()
            );
        }
        Commands::Pause => {
            println!("  {}", "(detached pause not yet implemented)".dimmed());
        }
        Commands::Resume => {
            println!("  {}", "(detached resume not yet implemented)".dimmed());
        }
        Commands::Stop => {
            println!("  {}", "(detached stop not yet implemented)".dimmed());
        }
        Commands::Volume { level } => {
            println!(
                "  {}",
                format!("(detached volume set to {} not yet implemented)", level).dimmed()
            );
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

// ─── Search & Play ───────────────────────────────────────────

async fn cmd_search(query: &str, limit: usize, config: &Config, db: &Database) -> Result<()> {
    let mut current_query = query.to_string();
    let limit = if limit > 0 {
        limit
    } else {
        config.player.search_results
    };

    loop {
        println!("\n  {} {}\n", "🔍 Searching:".bold(), current_query);

        let yt = YtDlp::new();
        let results = yt.search(&current_query, limit).await?;

        let items: Vec<String> = results
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let duration = v
                    .duration
                    .map(|d| youtube::types::format_duration(d as u64))
                    .unwrap_or_else(|| "LIVE".to_string());
                let channel = v.channel.as_deref().unwrap_or("Unknown");
                format!("{}. {} — {} [{}]", i + 1, v.title, channel, duration)
            })
            .collect();

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select a video to play")
            .items(&items)
            .default(0)
            .interact_opt()?;

        match selection {
            Some(idx) => {
                let video = &results[idx];
                match play_video(video, config, db).await? {
                    Some(new_query) => {
                        current_query = new_query;
                        continue;
                    }
                    None => break,
                }
            }
            None => {
                println!("  {}", "Cancelled.".dimmed());
                break;
            }
        }
    }

    Ok(())
}

async fn cmd_play(url: &str, config: &Config, db: &Database) -> Result<()> {
    let yt = YtDlp::new();

    println!("\n  {} {}\n", "🔍 Fetching:".bold(), url);

    let results = yt.search(url, 1).await?;
    if let Some(video) = results.first() {
        play_video(video, config, db).await?;
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

    // Fetch transcript
    println!("  {} fetching transcript...", "📝".dimmed());
    let transcript = fetch_transcript(&video.url).await.unwrap_or(None);
    match &transcript {
        Some(t) => println!(
            "  {} transcript loaded ({} segments, lang: {})",
            "✅".green(),
            t.segments.len(),
            t.language
        ),
        None => println!("  {} no transcript available", "⚠️".yellow()),
    }

    // Create AI context
    let mut ai_context = VideoContext::new(video.clone(), transcript);

    println!();
    println!(
        "  {} pause  {} seek ±10s  {} chat  {} fav  {} add queue  {} search  {} quit",
        "[space]".cyan(),
        "[←/→]".cyan(),
        "[c]".cyan(),
        "[f]".cyan(),
        "[a]".cyan(),
        "[s]".cyan(),
        "[q]".cyan()
    );
    println!();

    // Get stream URL and play
    let stream = yt.get_stream_url(&video.url).await?;

    let mut player = MpvPlayer::new();
    player.play(&stream.audio_url, &video.title).await?;

    // Enter interactive mode
    let result = loop {
        match run_interactive(&mut player, video).await? {
            InteractiveAction::Quit => {
                player.stop().await?;
                println!("  {} 👋", "Stopped.".dimmed());
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

                println!(
                    "\n  {} back to player  {} pause  {} seek  {} quit\n",
                    "🎵".green(),
                    "[space]".cyan(),
                    "[←/→]".cyan(),
                    "[q]".cyan()
                );
            }
            InteractiveAction::ToggleFavorite => {
                crossterm::terminal::disable_raw_mode().ok();
                let is_fav = library::favorites::is_favorite(db, &video.id).unwrap_or(false);
                if is_fav {
                    library::favorites::remove_favorite(db, &video.id)?;
                    println!("\r  {} removed from favorites   ", "💔".dimmed());
                } else {
                    library::favorites::add_favorite(db, video)?;
                    println!("\r  {} added to favorites!   ", "❤️");
                }
            }
            InteractiveAction::AddToQueue => {
                crossterm::terminal::disable_raw_mode().ok();
                library::queue::add_to_queue(db, video)?;
                let len = library::queue::queue_length(db)?;
                println!("\r  {} added to queue (#{})   ", "📋", len);
            }
        }
    };

    // Record to history
    let listened_secs = started_at.elapsed().as_secs();
    library::history::add_to_history(db, video, listened_secs)?;

    Ok(result)
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

    println!("  {} /quit to exit chat\n", "💡".dimmed());

    loop {
        print!("  {} ", "You:".bold().green());
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input.is_empty() {
            continue;
        }
        if input == "/quit" || input == "/q" {
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

        match ai_chat(context, input, &resolved).await {
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

    Ok(())
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
                library::queue::add_to_queue(db, video)?;
                let len = library::queue::queue_length(db)?;
                println!("  {} {} added to queue (#{})", "📋", video.title.bold(), len);
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
    use tui::app::{App, AppMode, NowPlaying};
    use tui::ui;

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let yt = YtDlp::new();
    let mut player: Option<MpvPlayer> = None;

    loop {
        // Draw
        terminal.draw(|frame| {
            ui::draw(frame, &app);
        })?;

        // Update playback info
        if let Some(ref p) = player {
            let pos = p.get_position().await.map(|d| d.as_secs()).unwrap_or(0);
            let dur = p.get_duration().await.map(|d| d.as_secs()).unwrap_or(0);
            let vol = p.get_property_volume().await.unwrap_or(80);

            // Check if still playing (detect end of track)
            let paused = if let Some(ref np) = app.now_playing {
                np.paused
            } else {
                false
            };

            app.update_playback(pos, dur, paused, vol);
        }

        // Handle events
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(KeyEvent {
                code, modifiers, ..
            }) = event::read()?
            {
                match app.mode {
                    AppMode::Search => match code {
                        KeyCode::Esc => {
                            app.should_quit = true;
                        }
                        KeyCode::Char('?') => {
                            app.mode = AppMode::Help;
                        }
                        KeyCode::Enter => {
                            if !app.search_input.is_empty() {
                                app.set_status("Searching...");
                                terminal.draw(|f| ui::draw(f, &app))?;

                                match yt.search(&app.search_input, 10).await {
                                    Ok(results) => {
                                        app.search_results = results;
                                        app.selected_index = 0;
                                        app.mode = AppMode::Results;
                                        app.set_status(format!(
                                            "Found {} results",
                                            app.search_results.len()
                                        ));
                                    }
                                    Err(e) => {
                                        app.set_status(format!("Search failed: {}", e));
                                    }
                                }
                            }
                        }
                        KeyCode::Backspace => {
                            app.search_input.pop();
                        }
                        KeyCode::Char(c) => {
                            app.search_input.push(c);
                        }
                        _ => {}
                    },

                    AppMode::Results => match code {
                        KeyCode::Esc | KeyCode::Char('q') => {
                            app.mode = AppMode::Search;
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            app.select_prev();
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            app.select_next();
                        }
                        KeyCode::Enter => {
                            if let Some(video) = app.search_results.get(app.selected_index).cloned()
                            {
                                app.set_status(format!("Loading: {}...", video.title));
                                terminal.draw(|f| ui::draw(f, &app))?;

                                match yt.get_stream_url(&video.url).await {
                                    Ok(stream) => {
                                        let mut p = MpvPlayer::new();
                                        if p.play(&stream.audio_url, &video.title).await.is_ok() {
                                            let is_fav = library::favorites::is_favorite(db, &video.id)
                                                .unwrap_or(false);

                                            app.now_playing = Some(NowPlaying {
                                                video: video.clone(),
                                                position_secs: 0,
                                                duration_secs: video.duration.unwrap_or(0.0) as u64,
                                                paused: false,
                                                volume: 80,
                                                is_fav,
                                            });

                                            // Record history
                                            library::history::add_to_history(db, &video, 0).ok();

                                            player = Some(p);
                                            app.mode = AppMode::Playing;
                                            app.set_status(format!("Playing: {}", video.title));
                                        }
                                    }
                                    Err(e) => {
                                        app.set_status(format!("Failed to play: {}", e));
                                    }
                                }
                            }
                        }
                        KeyCode::Char('/') => {
                            app.mode = AppMode::Search;
                            app.search_input.clear();
                        }
                        _ => {}
                    },

                    AppMode::Playing => match (code, modifiers) {
                        (KeyCode::Esc, _) | (KeyCode::Char('q'), _) => {
                            if let Some(mut p) = player.take() {
                                p.stop().await.ok();
                            }
                            app.now_playing = None;
                            app.mode = AppMode::Search;
                            app.set_status("Playback stopped");
                        }
                        (KeyCode::Char(' '), _) => {
                            if let Some(ref p) = player {
                                p.toggle_pause().await.ok();
                                if let Some(ref mut np) = app.now_playing {
                                    np.paused = !np.paused;
                                }
                            }
                        }
                        (KeyCode::Left, _) => {
                            if let Some(ref p) = player {
                                p.seek(-10.0).await.ok();
                            }
                        }
                        (KeyCode::Right, _) => {
                            if let Some(ref p) = player {
                                p.seek(10.0).await.ok();
                            }
                        }
                        (KeyCode::Up, _) => {
                            if let Some(ref p) = player {
                                if let Ok(vol) = p.get_property_volume().await {
                                    let new = (vol + 5).min(100);
                                    p.set_volume(new).await.ok();
                                }
                            }
                        }
                        (KeyCode::Down, _) => {
                            if let Some(ref p) = player {
                                if let Ok(vol) = p.get_property_volume().await {
                                    let new = vol.saturating_sub(5);
                                    p.set_volume(new).await.ok();
                                }
                            }
                        }
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
                        (KeyCode::Char('a'), _) => {
                            if let Some(ref np) = app.now_playing {
                                library::queue::add_to_queue(db, &np.video).ok();
                                let len = library::queue::queue_length(db).unwrap_or(0);
                                app.set_status(format!("Added to queue (#{})", len));
                            }
                        }
                        (KeyCode::Char('s') | KeyCode::Char('/'), _) => {
                            app.mode = AppMode::Search;
                            app.search_input.clear();
                        }
                        _ => {}
                    },

                    AppMode::Help => {
                        // Any key goes back
                        app.mode = AppMode::Search;
                    }

                    _ => {
                        if code == KeyCode::Esc {
                            app.mode = AppMode::Search;
                        }
                    }
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
