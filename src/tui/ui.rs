use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Table, TableState, Tabs,
    },
    Frame,
};

use super::app::{App, NowPlaying, Panel};
use crate::media::types::format_duration;

// ── Color Palette (Premium dark theme) ─────────────────────────────────────
const BRAND: Color = Color::Rgb(30, 215, 96); // Vibrant green
const BRAND_DIM: Color = Color::Rgb(20, 145, 65); // Muted green for borders
const ACCENT: Color = Color::Rgb(80, 210, 200); // Teal highlight
const WARN: Color = Color::Rgb(255, 200, 60); // Amber
const DIM: Color = Color::Rgb(75, 75, 100); // Muted gray-blue
const TEXT: Color = Color::Rgb(230, 230, 245); // Soft white
const TEXT_DIM: Color = Color::Rgb(130, 130, 155); // Muted text
const SELECTED: Color = Color::Rgb(30, 215, 96); // Selection = brand
const LOVE: Color = Color::Rgb(255, 75, 110); // Heart pink-red
const KEY_BG: Color = Color::Rgb(55, 55, 75); // Key badge background
const SURFACE: Color = Color::Rgb(40, 40, 58); // Selected row background
const ROW_ALT: Color = Color::Rgb(28, 28, 42); // Alternating row background

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header + tabs
            Constraint::Min(3),    // Body panel
            Constraint::Length(5), // Now playing (progress + 2-line info)
            Constraint::Length(2), // Keybind bar
        ])
        .split(area);

    draw_header(frame, chunks[0], app);
    draw_body(frame, chunks[1], app);
    draw_now_playing_bar(frame, chunks[2], app);
    draw_keybind_bar(frame, chunks[3], app);

    // ── Modal overlay: playlist picker ──────────────────────────────
    if app.playlist_picker.is_some() {
        draw_playlist_picker(frame, area, app);
    }
}

// ── Header / Tabs ──────────────────────────────────────────────────────────

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(14), Constraint::Min(10)])
        .split(area);

    // Brand
    let brand = Paragraph::new(Line::from(vec![
        Span::styled("  🎵 ", Style::default()),
        Span::styled("aux", Style::default().fg(BRAND).bold()),
        Span::styled(" ", Style::default()),
    ]))
    .block(
        Block::default()
            .borders(Borders::BOTTOM | Borders::RIGHT)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(DIM)),
    );
    frame.render_widget(brand, layout[0]);

    // Tabs
    let tab_titles = vec![
        "🔍Search",
        "📋Results",
        "🎵Lyrics",
        "📦Queue",
        "❤Favs",
        "⏱History",
        "🎶Lists",
        "💬Chat",
        "❓Help",
    ];
    let selected = match app.panel {
        Panel::Search => 0,
        Panel::Results => 1,
        Panel::Lyrics => 2,
        Panel::Queue => 3,
        Panel::Favorites => 4,
        Panel::History => 5,
        Panel::Playlists => 6,
        Panel::Chat => 7,
        Panel::Help => 8,
    };

    let tabs = Tabs::new(tab_titles)
        .select(selected)
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(DIM)),
        )
        .style(Style::default().fg(DIM))
        .highlight_style(
            Style::default()
                .fg(BRAND)
                .bold()
                .add_modifier(Modifier::UNDERLINED),
        );

    frame.render_widget(tabs, layout[1]);
}

// ── Body dispatch ──────────────────────────────────────────────────────────

fn draw_body(frame: &mut Frame, area: Rect, app: &App) {
    match app.panel {
        Panel::Search => draw_search(frame, area, app),
        Panel::Results => draw_results(frame, area, app),
        Panel::Lyrics => draw_lyrics(frame, area, app),
        Panel::Queue => draw_queue(frame, area, app),
        Panel::Favorites => draw_favorites(frame, area, app),
        Panel::History => draw_history(frame, area, app),
        Panel::Playlists => draw_playlists(frame, area, app),
        Panel::Chat => draw_chat(frame, area, app),
        Panel::Help => draw_help(frame, area, app),
    }
}

// ── Playlist picker popup (overlay) ─────────────────────────────────────────

fn draw_playlist_picker(frame: &mut Frame, area: Rect, app: &App) {
    let picker = match app.playlist_picker {
        Some(ref pk) => pk,
        None => return,
    };

    // Center popup: 50 wide, min(playlists+4, 60% of screen) tall
    let popup_w = 50u16.min(area.width.saturating_sub(4));
    let popup_h = (picker.playlists.len() as u16 + 5)
        .min(area.height * 60 / 100)
        .max(6);
    let x = (area.width.saturating_sub(popup_w)) / 2;
    let y = (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = Rect::new(x, y, popup_w, popup_h);

    // Clear the area behind the popup
    frame.render_widget(Clear, popup_area);

    // Truncate video title for the block title
    let max_title = (popup_w as usize).saturating_sub(20);
    let vtitle: String = picker.video.title.chars().take(max_title).collect();
    let title = format!(" \u{1F3A7} Add \u{201C}{}\u{201D} to playlist ", vtitle);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BRAND))
        .style(Style::default().bg(Color::Rgb(18, 18, 28)));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    if picker.playlists.is_empty() {
        let msg = Paragraph::new("  No playlists yet. Create one in the Playlists tab ([n]).")
            .style(Style::default().fg(DIM));
        frame.render_widget(msg, inner);
        return;
    }

    // Render playlist rows
    let rows: Vec<Row> = picker
        .playlists
        .iter()
        .enumerate()
        .map(|(i, pl)| {
            let sel = i == picker.selected;
            let prefix = if sel { "▸ " } else { "  " };
            let style = if sel {
                Style::default()
                    .fg(BRAND)
                    .bg(SURFACE)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT)
            };
            Row::new(vec![
                Cell::from(format!("{}{}", prefix, pl.name)),
                Cell::from(format!("{} tracks", pl.item_count)),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        rows,
        [Constraint::Percentage(70), Constraint::Percentage(30)],
    )
    .row_highlight_style(Style::default().fg(BRAND));

    frame.render_widget(table, inner);

    // Footer hint
    let footer_y = popup_area.y + popup_area.height - 1;
    if footer_y < area.height {
        let hint = Paragraph::new(" [↑↓] select  [Enter] add  [Esc] cancel ")
            .style(Style::default().fg(DIM));
        let footer_area = Rect::new(popup_area.x + 1, footer_y, popup_w - 2, 1);
        frame.render_widget(hint, footer_area);
    }
}

// ── Search panel ───────────────────────────────────────────────────────────

fn draw_search(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Length(3), Constraint::Min(3)])
        .split(area);

    // Build source selector label: [▶YouTube | ☁SoundCloud | ♫YT Music]
    let sources = crate::media::Source::searchable();
    let source_parts: Vec<String> = sources
        .iter()
        .map(|s| {
            if s == &app.search_source {
                format!("*{}{}*", s.icon(), s.display_name())
            } else {
                format!("{}{}", s.icon(), s.display_name())
            }
        })
        .collect();
    let source_selector = source_parts.join(" | ");

    // Show history position in title when navigating
    let title = if let Some(idx) = app.search_history_index {
        format!(
            " Search [{}]  ·  history {}/{} ",
            source_selector,
            idx + 1,
            app.search_history.len()
        )
    } else if !app.search_history.is_empty() {
        format!(
            " Search [{}]  ·  {} {} ",
            source_selector,
            app.search_history.len(),
            if app.search_history.len() == 1 {
                "query"
            } else {
                "queries"
            }
        )
    } else {
        format!(" Search [{}] ", source_selector)
    };

    let border_color = if app.search_history_index.is_some() {
        WARN
    } else {
        ACCENT
    };

    let input = Paragraph::new(Line::from(vec![
        Span::styled("🔍 ", Style::default()),
        Span::styled(&app.search_input, Style::default().fg(TEXT).bold()),
        Span::styled("▌", Style::default().fg(ACCENT)),
    ]))
    .block(
        Block::default()
            .title(title)
            .title_style(Style::default().fg(border_color).bold())
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color)),
    );
    frame.render_widget(input, chunks[0]);

    // Second line shows search history or generic hint
    let hint_spans: Vec<Span> =
        if !app.search_history.is_empty() && app.search_history_index.is_none() {
            let preview = app
                .search_history
                .first()
                .map(|s| {
                    if s.len() > 30 {
                        format!("{}…", &s[..30])
                    } else {
                        s.clone()
                    }
                })
                .unwrap_or_default();
            vec![
                Span::styled("  Last: ", Style::default().fg(DIM)),
                Span::styled(preview, Style::default().fg(WARN)),
                Span::styled("  ↑/↓ recall  ", Style::default().fg(DIM)),
                Span::styled("Ctrl+S", Style::default().fg(ACCENT)),
                Span::styled(" source  ", Style::default().fg(DIM)),
                Span::styled("Enter", Style::default().fg(ACCENT)),
                Span::styled(" search  ", Style::default().fg(DIM)),
                Span::styled("Tab", Style::default().fg(ACCENT)),
                Span::styled(" panels", Style::default().fg(DIM)),
            ]
        } else {
            vec![
                Span::styled("  Type to search  ", Style::default().fg(DIM)),
                Span::styled("Ctrl+S", Style::default().fg(ACCENT)),
                Span::styled(" change source  ", Style::default().fg(DIM)),
                Span::styled("Enter", Style::default().fg(ACCENT)),
                Span::styled(" search  ", Style::default().fg(DIM)),
                Span::styled("Tab", Style::default().fg(ACCENT)),
                Span::styled(" panels  ", Style::default().fg(DIM)),
                Span::styled("?", Style::default().fg(ACCENT)),
                Span::styled(" help  ", Style::default().fg(DIM)),
                Span::styled("q", Style::default().fg(ACCENT)),
                Span::styled(" quit", Style::default().fg(DIM)),
            ]
        };

    let hint = Paragraph::new(vec![Line::from(""), Line::from(hint_spans)]);
    frame.render_widget(hint, chunks[1]);
}

// ── Results panel ─────────────────────────────────────────────────────────

fn draw_results(frame: &mut Frame, area: Rect, app: &App) {
    let rows: Vec<Row> = app
        .search_results
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let global_idx = app.search_global_index(i) + 1;
            let duration = v
                .duration
                .map(|d| format_duration(d as u64))
                .unwrap_or_else(|| "LIVE".to_string());
            let channel = v.channel.as_deref().unwrap_or("Unknown");
            let selected = i == app.selected_index;

            let row_bg = if selected {
                SURFACE
            } else if i % 2 == 1 {
                ROW_ALT
            } else {
                Color::Reset
            };
            let text_style = if selected {
                Style::default().fg(SELECTED).bold().bg(row_bg)
            } else {
                Style::default().fg(TEXT).bg(row_bg)
            };
            let prefix = if selected { "▸" } else { " " };

            Row::new(vec![
                Cell::from(format!(" {}{}", prefix, global_idx)).style(
                    Style::default()
                        .fg(if selected { BRAND } else { DIM })
                        .bg(row_bg),
                ),
                Cell::from(v.title.as_str()).style(text_style),
                Cell::from(channel).style(Style::default().fg(TEXT_DIM).bg(row_bg)),
                Cell::from(duration).style(Style::default().fg(DIM).bg(row_bg)),
            ])
        })
        .collect();

    let title = format!(
        " Results ({})  ·  page {}/{} ",
        app.all_search_results.len(),
        app.search_page + 1,
        app.search_total_pages(),
    );
    let block = Block::default()
        .title(title)
        .title_style(Style::default().fg(ACCENT).bold())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(DIM));

    let table = Table::new(
        rows,
        [
            Constraint::Length(5),
            Constraint::Percentage(55),
            Constraint::Percentage(30),
            Constraint::Length(7),
        ],
    )
    .block(block)
    .header(
        Row::new(vec![" #", "Title", "Channel", "Time"])
            .style(Style::default().fg(DIM).add_modifier(Modifier::BOLD))
            .bottom_margin(0),
    );
    let mut table_state = TableState::default().with_selected(Some(app.selected_index));
    frame.render_stateful_widget(table, area, &mut table_state);

    // Scrollbar
    if app.search_results.len() > 1 {
        let mut sb_state =
            ScrollbarState::new(app.search_results.len()).position(app.selected_index);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .track_symbol(Some("│"))
                .thumb_symbol("█"),
            area.inner(ratatui::layout::Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut sb_state,
        );
    }
}

// ── Lyrics panel ─────────────────────────────────────────────────────────────

fn draw_lyrics(frame: &mut Frame, area: Rect, app: &App) {
    let transcript = match &app.transcript {
        Some(t) if !t.segments.is_empty() => t,
        _ => {
            // Empty state
            let empty = Paragraph::new(vec![
                Line::from(""),
                Line::from(vec![Span::styled(
                    "  No transcript available.",
                    Style::default().fg(DIM),
                )]),
                Line::from(""),
                Line::from(vec![Span::styled(
                    "  Try asking the AI in Chat! 💬",
                    Style::default().fg(ACCENT),
                )]),
            ])
            .block(
                Block::default()
                    .title(" Lyrics 🎵 ")
                    .title_style(Style::default().fg(ACCENT).bold())
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(DIM)),
            );
            frame.render_widget(empty, area);
            return;
        }
    };

    // Description fallback: show as plain text block
    if transcript.language == "description" {
        let text = transcript
            .segments
            .first()
            .map(|s| s.text.as_str())
            .unwrap_or("");
        let widget = Paragraph::new(text)
            .wrap(ratatui::widgets::Wrap { trim: false })
            .scroll((app.lyrics_scroll, 0))
            .block(
                Block::default()
                    .title(" Description 📄 (no subtitles) ")
                    .title_style(Style::default().fg(WARN).bold())
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(DIM)),
            );
        frame.render_widget(widget, area);
        return;
    }

    let pos_secs = app
        .now_playing
        .as_ref()
        .map(|np| np.position_secs)
        .unwrap_or(0);

    // Build lines with timestamp + text, color-coded by position
    let mut lines: Vec<Line> = Vec::with_capacity(transcript.segments.len() + 2);
    let mut current_line_idx: usize = 0;

    for (i, seg) in transcript.segments.iter().enumerate() {
        let seg_start = seg.start.as_secs();
        // Use ceiling for end time to avoid gaps at fractional second boundaries
        let seg_end = (seg.end.as_millis() as u64 + 999) / 1000; // ceil
        let is_current = pos_secs >= seg_start && pos_secs < seg_end;
        let is_past = pos_secs >= seg_end;

        if is_current {
            current_line_idx = i;
        }

        let timestamp = format!("  [{:02}:{:02}] ", seg_start / 60, seg_start % 60);

        let (ts_style, text_style) = if is_current {
            (
                Style::default().fg(BRAND).bold(),
                Style::default().fg(BRAND).bold(),
            )
        } else if is_past {
            (Style::default().fg(DIM), Style::default().fg(DIM))
        } else {
            (Style::default().fg(DIM), Style::default().fg(TEXT))
        };

        let prefix = if is_current { "▶ " } else { "  " };

        lines.push(Line::from(vec![
            Span::styled(
                prefix,
                if is_current {
                    Style::default().fg(BRAND)
                } else {
                    Style::default()
                },
            ),
            Span::styled(timestamp, ts_style),
            Span::styled(&seg.text, text_style),
        ]));
    }

    // Compute scroll position
    let visible_h = area.height.saturating_sub(2) as usize; // -2 for borders
    let scroll = if app.lyrics_auto_scroll {
        // Auto-scroll: center current segment
        if current_line_idx >= visible_h / 2 {
            (current_line_idx - visible_h / 2) as u16
        } else {
            0
        }
    } else {
        app.lyrics_scroll
    };

    let title = if app.lyrics_auto_scroll {
        format!(
            " Lyrics 🎵 ({} segments) · auto ",
            transcript.segments.len()
        )
    } else {
        format!(
            " Lyrics 🎵 ({} segments) · manual ",
            transcript.segments.len()
        )
    };

    let widget = Paragraph::new(lines).scroll((scroll, 0)).block(
        Block::default()
            .title(title)
            .title_style(Style::default().fg(ACCENT).bold())
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(if app.lyrics_auto_scroll { DIM } else { ACCENT })),
    );
    frame.render_widget(widget, area);
}

// ── Queue panel ───────────────────────────────────────────────────────────

fn draw_queue(frame: &mut Frame, area: Rect, app: &App) {
    if app.queue_items.is_empty() {
        let empty = Paragraph::new(vec![
            Line::from(""),
            Line::from(vec![Span::styled(
                "  Queue is empty. Press [a] while browsing results to add tracks.",
                Style::default().fg(DIM),
            )]),
        ])
        .block(
            Block::default()
                .title(" Queue ")
                .title_style(Style::default().fg(ACCENT).bold())
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(DIM)),
        );
        frame.render_widget(empty, area);
        return;
    }

    let rows: Vec<Row> = app
        .queue_items
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let selected = i == app.selected_index;
            let row_bg = if selected {
                SURFACE
            } else if i % 2 == 1 {
                ROW_ALT
            } else {
                Color::Reset
            };
            let text_style = if selected {
                Style::default().fg(SELECTED).bold().bg(row_bg)
            } else {
                Style::default().fg(TEXT).bg(row_bg)
            };
            let prefix = if selected { "▸" } else { " " };
            let dur = e
                .duration_secs
                .map(|d| format_duration(d as u64))
                .unwrap_or_else(|| "??:??".to_string());
            let title_display = if let Some(&pos) = app.saved_positions.get(&e.video_id) {
                format!("{} ⏸ {}:{:02}", e.title, pos / 60, pos % 60)
            } else {
                e.title.clone()
            };
            Row::new(vec![
                Cell::from(format!(" {}{}", prefix, i + 1)).style(
                    Style::default()
                        .fg(if selected { BRAND } else { DIM })
                        .bg(row_bg),
                ),
                Cell::from(title_display).style(text_style),
                Cell::from(dur).style(Style::default().fg(DIM).bg(row_bg)),
            ])
        })
        .collect();

    let title = format!(" Queue ({}) ", app.queue_items.len());
    let block = Block::default()
        .title(title)
        .title_style(Style::default().fg(ACCENT).bold())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(DIM));
    let table = Table::new(
        rows,
        [
            Constraint::Length(5),
            Constraint::Percentage(80),
            Constraint::Length(7),
        ],
    )
    .block(block)
    .header(
        Row::new(vec![" #", "Title", "Time"])
            .style(Style::default().fg(DIM).add_modifier(Modifier::BOLD)),
    );
    let mut table_state = TableState::default().with_selected(Some(app.selected_index));
    frame.render_stateful_widget(table, area, &mut table_state);

    // Scrollbar
    if app.queue_items.len() > 1 {
        let mut sb = ScrollbarState::new(app.queue_items.len()).position(app.selected_index);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .track_symbol(Some("│"))
                .thumb_symbol("█"),
            area.inner(ratatui::layout::Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut sb,
        );
    }
}

// ── Favorites panel ───────────────────────────────────────────────────────

fn draw_favorites(frame: &mut Frame, area: Rect, app: &App) {
    if app.fav_items.is_empty() {
        let empty = Paragraph::new(vec![
            Line::from(""),
            Line::from(vec![Span::styled(
                "  No favorites yet. Press [f] while playing to add tracks you love.",
                Style::default().fg(DIM),
            )]),
        ])
        .block(
            Block::default()
                .title(" Favorites ❤️ ")
                .title_style(Style::default().fg(LOVE).bold())
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(DIM)),
        );
        frame.render_widget(empty, area);
        return;
    }

    let rows: Vec<Row> = app
        .fav_items
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let selected = i == app.selected_index;
            let row_bg = if selected {
                SURFACE
            } else if i % 2 == 1 {
                ROW_ALT
            } else {
                Color::Reset
            };
            let text_style = if selected {
                Style::default().fg(SELECTED).bold().bg(row_bg)
            } else {
                Style::default().fg(TEXT).bg(row_bg)
            };
            let prefix = if selected { "▸" } else { " " };
            let dur = e
                .duration_secs
                .map(|d| format_duration(d as u64))
                .unwrap_or_else(|| "??:??".to_string());
            let ch = e.channel.as_deref().unwrap_or("Unknown");
            let title_display = if let Some(&pos) = app.saved_positions.get(&e.video_id) {
                format!("{} ⏸ {}:{:02}", e.title, pos / 60, pos % 60)
            } else {
                e.title.clone()
            };
            Row::new(vec![
                Cell::from(format!(" {}❤", prefix)).style(
                    Style::default()
                        .fg(if selected { LOVE } else { LOVE })
                        .bg(row_bg),
                ),
                Cell::from(title_display).style(text_style),
                Cell::from(ch).style(Style::default().fg(TEXT_DIM).bg(row_bg)),
                Cell::from(dur).style(Style::default().fg(DIM).bg(row_bg)),
            ])
        })
        .collect();

    let title = format!(" Favorites ❤️ ({}) ", app.fav_items.len());
    let block = Block::default()
        .title(title)
        .title_style(Style::default().fg(LOVE).bold())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(DIM));
    let table = Table::new(
        rows,
        [
            Constraint::Length(4),
            Constraint::Percentage(50),
            Constraint::Percentage(30),
            Constraint::Length(7),
        ],
    )
    .block(block)
    .header(
        Row::new(vec![" ❤", "Title", "Channel", "Time"])
            .style(Style::default().fg(DIM).add_modifier(Modifier::BOLD)),
    );
    let mut table_state = TableState::default().with_selected(Some(app.selected_index));
    frame.render_stateful_widget(table, area, &mut table_state);

    // Scrollbar
    if app.fav_items.len() > 1 {
        let mut sb = ScrollbarState::new(app.fav_items.len()).position(app.selected_index);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .track_symbol(Some("│"))
                .thumb_symbol("█"),
            area.inner(ratatui::layout::Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut sb,
        );
    }
}

// ── History panel ──────────────────────────────────────────────────────────

fn draw_history(frame: &mut Frame, area: Rect, app: &App) {
    if app.history_items.is_empty() {
        let empty = Paragraph::new(vec![
            Line::from(""),
            Line::from(vec![Span::styled(
                "  No listening history yet.",
                Style::default().fg(DIM),
            )]),
        ])
        .block(
            Block::default()
                .title(" History ")
                .title_style(Style::default().fg(ACCENT).bold())
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(DIM)),
        );
        frame.render_widget(empty, area);
        return;
    }

    // Track which video_ids already showed position so only the first (most recent) entry gets it
    let mut seen_position: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let rows: Vec<Row> = app
        .history_items
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let selected = i == app.selected_index;
            let row_bg = if selected {
                SURFACE
            } else if i % 2 == 1 {
                ROW_ALT
            } else {
                Color::Reset
            };
            let text_style = if selected {
                Style::default().fg(SELECTED).bold().bg(row_bg)
            } else {
                Style::default().fg(TEXT).bg(row_bg)
            };
            let prefix = if selected { "\u{25b8}" } else { " " };
            let ch = e.channel.as_deref().unwrap_or("Unknown");
            let when = e.played_at.split('T').next().unwrap_or(&e.played_at);
            // Show saved position only on the first (most recent) entry per video
            let title_display = if let Some(&pos) = app.saved_positions.get(&e.video_id) {
                if seen_position.insert(&e.video_id) {
                    format!("{} \u{23f8} {}:{:02}", e.title, pos / 60, pos % 60)
                } else {
                    e.title.clone()
                }
            } else {
                e.title.clone()
            };
            Row::new(vec![
                Cell::from(format!(" {}{}", prefix, i + 1)).style(
                    Style::default()
                        .fg(if selected { BRAND } else { DIM })
                        .bg(row_bg),
                ),
                Cell::from(title_display).style(text_style),
                Cell::from(ch).style(Style::default().fg(TEXT_DIM).bg(row_bg)),
                Cell::from(when).style(Style::default().fg(DIM).bg(row_bg)),
            ])
        })
        .collect();

    let title = format!(" History ({}) ", app.history_items.len());
    let block = Block::default()
        .title(title)
        .title_style(Style::default().fg(ACCENT).bold())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(DIM));
    let table = Table::new(
        rows,
        [
            Constraint::Length(5),
            Constraint::Percentage(50),
            Constraint::Percentage(30),
            Constraint::Length(10),
        ],
    )
    .block(block)
    .header(
        Row::new(vec![" #", "Title", "Channel", "Date"])
            .style(Style::default().fg(DIM).add_modifier(Modifier::BOLD)),
    );
    let mut table_state = TableState::default().with_selected(Some(app.selected_index));
    frame.render_stateful_widget(table, area, &mut table_state);

    // Scrollbar
    if app.history_items.len() > 1 {
        let mut sb = ScrollbarState::new(app.history_items.len()).position(app.selected_index);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .track_symbol(Some("│"))
                .thumb_symbol("█"),
            area.inner(ratatui::layout::Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut sb,
        );
    }
}

// ── Playlists panel ─────────────────────────────────────────────────────────

fn draw_playlists(frame: &mut Frame, area: Rect, app: &App) {
    if let Some((ref name, ref items)) = app.playlist_items_view {
        // Detail view: show items in a specific playlist
        let title = format!(
            " 🎶 {} ({} tracks) — [Esc] back, [Enter] play ",
            name,
            items.len()
        );
        let rows: Vec<Row> = items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let style = if i == app.selected_index {
                    Style::default().fg(BRAND).bold()
                } else {
                    Style::default().fg(TEXT)
                };
                let ch = item.channel.as_deref().unwrap_or("Unknown");
                Row::new(vec![
                    Cell::from(format!(" {}", i + 1)).style(Style::default().fg(DIM)),
                    Cell::from(item.title.as_str()).style(style),
                    Cell::from(ch).style(Style::default().fg(DIM)),
                ])
            })
            .collect();

        let table = Table::new(
            rows,
            [
                Constraint::Length(4),
                Constraint::Percentage(70),
                Constraint::Percentage(25),
            ],
        )
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(DIM)),
        )
        .header(
            Row::new(vec![" #", "Title", "Channel"])
                .style(Style::default().fg(DIM).add_modifier(Modifier::BOLD)),
        );
        let mut table_state = TableState::default().with_selected(Some(app.selected_index));
        frame.render_stateful_widget(table, area, &mut table_state);
    } else {
        // List view: show all playlists
        // Split area if we have an active name input
        let (table_area, input_area) = if app.playlist_name_input.is_some() {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(3), Constraint::Length(3)])
                .split(area);
            (chunks[0], Some(chunks[1]))
        } else {
            (area, None)
        };

        let rows: Vec<Row> = app
            .playlist_list
            .iter()
            .enumerate()
            .map(|(i, pl)| {
                let style = if i == app.selected_index {
                    Style::default().fg(BRAND).bold()
                } else {
                    Style::default().fg(TEXT)
                };
                Row::new(vec![
                    Cell::from(format!(" {}", i + 1)).style(Style::default().fg(DIM)),
                    Cell::from(pl.name.as_str()).style(style),
                    Cell::from(format!("{} tracks", pl.item_count)).style(Style::default().fg(DIM)),
                ])
            })
            .collect();

        let title = if app.playlist_name_input.is_some() {
            " 🎶 Playlists — type name, [Enter] create, [Esc] cancel "
        } else {
            " 🎶 Playlists — [Enter] view, [p] play, [n] new, [d] delete "
        };

        let table = Table::new(
            rows,
            [
                Constraint::Length(4),
                Constraint::Percentage(60),
                Constraint::Percentage(30),
            ],
        )
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(DIM)),
        )
        .header(
            Row::new(vec![" #", "Name", "Tracks"])
                .style(Style::default().fg(DIM).add_modifier(Modifier::BOLD)),
        );
        let mut table_state = TableState::default().with_selected(Some(app.selected_index));
        frame.render_stateful_widget(table, table_area, &mut table_state);

        // Render name input if active
        if let Some(ref name) = app.playlist_name_input {
            let input = Paragraph::new(format!(" {}_", name))
                .style(Style::default().fg(TEXT))
                .block(
                    Block::default()
                        .title(" New playlist name ")
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(BRAND)),
                );
            frame.render_widget(input, input_area.unwrap());
        }
    }
}

// ── Chat panel ──────────────────────────────────────────────────────────────

fn draw_chat(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(3)])
        .split(area);

    // ── Message area ──────────────────────────────────────────
    let mut lines: Vec<Line> = Vec::new();

    if app.chat_messages.is_empty() {
        let has_track = app.now_playing.is_some();
        if has_track {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("  🎧 ", Style::default()),
                Span::styled(
                    "Chat about the current track! Ask questions, get summaries, or just vibe.",
                    Style::default().fg(DIM),
                ),
            ]));
        } else {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![Span::styled(
                "  Play a track first to start chatting about it.",
                Style::default().fg(DIM),
            )]));
        }
    } else {
        for msg in &app.chat_messages {
            let (icon, color) = if msg.role == "user" {
                ("🗣️", ACCENT)
            } else {
                ("🤖", BRAND)
            };

            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(format!("  {} ", icon), Style::default()),
                Span::styled(
                    if msg.role == "user" { "You" } else { "Aux" },
                    Style::default().fg(color).bold(),
                ),
            ]));

            // Wrap message text to available width
            let max_w = area.width.saturating_sub(6) as usize;
            for line_text in msg.content.lines() {
                // Simple char-based wrapping
                let chars: Vec<char> = line_text.chars().collect();
                if chars.is_empty() {
                    lines.push(Line::from(Span::styled("    ", Style::default())));
                } else {
                    for chunk in chars.chunks(max_w.max(20)) {
                        let s: String = chunk.iter().collect();
                        lines.push(Line::from(Span::styled(
                            format!("    {}", s),
                            Style::default().fg(TEXT),
                        )));
                    }
                }
            }
        }
    }

    if app.chat_loading {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  🤖 ", Style::default()),
            Span::styled("Thinking...", Style::default().fg(WARN).italic()),
        ]));
    }

    // Compute scroll: show the bottom of the conversation
    let visible_h = chunks[0].height.saturating_sub(2) as usize; // -2 for border
    let total_lines = lines.len();
    let scroll = if total_lines > visible_h {
        (total_lines - visible_h) as u16 - app.chat_scroll.min((total_lines - visible_h) as u16)
    } else {
        0
    };

    let title = format!(" Chat ({} messages) ", app.chat_messages.len());
    let messages_widget = Paragraph::new(lines).scroll((scroll, 0)).block(
        Block::default()
            .title(title)
            .title_style(Style::default().fg(ACCENT).bold())
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(DIM)),
    );
    frame.render_widget(messages_widget, chunks[0]);

    // ── Input area ───────────────────────────────────────────
    let input_widget = Paragraph::new(Line::from(vec![
        Span::styled("\u{1F4AC} ", Style::default()),
        Span::styled(&app.chat_input, Style::default().fg(TEXT).bold()),
        Span::styled("\u{258C}", Style::default().fg(ACCENT)),
    ]))
    .block(
        Block::default()
            .title(" Ask anything ")
            .title_style(Style::default().fg(BRAND).bold())
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(ACCENT)),
    );
    frame.render_widget(input_widget, chunks[1]);
}

// ── Help panel ─────────────────────────────────────────────────────────────

fn draw_help(frame: &mut Frame, area: Rect, app: &App) {
    let lines = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "  Navigation",
            Style::default().fg(BRAND).bold(),
        )]),
        Line::from(""),
        help_row("Tab", "Cycle panels"),
        help_row("/", "New search"),
        help_row("Enter", "Play / confirm / select"),
        help_row("\u{2191} \u{2193} / j k", "Navigate list items"),
        help_row("\u{2190} \u{2192}", "Page prev / next (Results panel)"),
        help_row("Esc", "Back / close sub-view"),
        help_row("q", "Back / quit"),
        help_row("?", "This help screen"),
        help_row("", "Videos auto-resume from last position"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  Playback (any panel while playing)",
            Style::default().fg(ACCENT).bold(),
        )]),
        Line::from(""),
        help_row("Space", "Pause / Resume"),
        help_row("\u{2190} \u{2192}", "Seek \u{00B1}10s (non-list panels)"),
        help_row("Shift+\u{2190} \u{2192}", "Seek \u{00B1}60s (any panel)"),
        help_row("+ / - (or =)", "Volume \u{00B1}5%"),
        help_row("] / [", "Speed up / down (0.25x\u{2013}4.0x)"),
        help_row("n", "Skip to next track"),
        help_row("p", "Restart / previous track"),
        help_row("r", "Cycle repeat (off \u{2192} one \u{2192} all)"),
        help_row("z", "Toggle shuffle"),
        help_row("f", "Toggle favorite"),
        help_row("a", "Add/remove current track to queue"),
        help_row("l", "Add selected video to playlist"),
        help_row("S", "Stop playback"),
        help_row(
            "t",
            "Sleep timer (15m \u{2192} 30m \u{2192} 1h \u{2192} 2h \u{2192} off)",
        ),
        help_row(
            "e",
            "Cycle equalizer (flat → bass-boost → vocal → treble → loudness)",
        ),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  Lyrics Panel",
            Style::default().fg(ACCENT).bold(),
        )]),
        Line::from(""),
        help_row("Shift+\u{2191} \u{2193}", "Scroll lyrics manually"),
        help_row("0", "Reset auto-scroll"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  Results Panel",
            Style::default().fg(ACCENT).bold(),
        )]),
        Line::from(""),
        help_row("a", "Add / remove from queue"),
        help_row("f", "Toggle favorite"),
        help_row("l", "Add to playlist"),
        help_row("\u{2190} \u{2192}", "Page prev / next"),
        help_row("/", "New search"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  Queue Panel",
            Style::default().fg(ACCENT).bold(),
        )]),
        Line::from(""),
        help_row("d", "Remove from queue"),
        help_row("l", "Add to playlist"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  Favorites Panel",
            Style::default().fg(ACCENT).bold(),
        )]),
        Line::from(""),
        help_row("d", "Unfavorite"),
        help_row("l", "Add to playlist"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  History Panel",
            Style::default().fg(ACCENT).bold(),
        )]),
        Line::from(""),
        help_row("l", "Add to playlist"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  Playlists Panel",
            Style::default().fg(ACCENT).bold(),
        )]),
        Line::from(""),
        help_row("Enter", "View playlist items / Play item"),
        help_row("n", "Create new playlist"),
        help_row("d", "Delete playlist / Remove item"),
        help_row("l", "Add selected video to playlist"),
        help_row("p", "Play entire playlist (load to queue)"),
        help_row("Esc", "Back to list / back to Search"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  Chat Panel",
            Style::default().fg(ACCENT).bold(),
        )]),
        Line::from(""),
        help_row("Enter", "Send message to AI"),
        help_row("\u{2191} / \u{2193}", "Scroll chat history"),
        help_row("Esc", "Back to Search"),
        Line::from(""),
    ];

    let total_lines = lines.len() as u16;

    let help = Paragraph::new(lines).scroll((app.help_scroll, 0)).block(
        Block::default()
            .title(" Help \u{2014} [\u{2191}\u{2193}] scroll  [Esc] back ")
            .title_style(Style::default().fg(BRAND).bold())
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(DIM)),
    );
    frame.render_widget(help, area);

    // Scrollbar
    let content_h = area.height.saturating_sub(2);
    if total_lines > content_h {
        let mut sb_state = ScrollbarState::new(total_lines.saturating_sub(content_h) as usize)
            .position(app.help_scroll as usize);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .thumb_style(Style::default().fg(BRAND))
                .track_style(Style::default().fg(DIM)),
            area,
            &mut sb_state,
        );
    }
}

fn help_row<'a>(key: &'a str, desc: &'a str) -> Line<'a> {
    Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(format!(" {} ", key), Style::default().fg(TEXT).bg(KEY_BG)),
        Span::styled(format!(" {}", desc), Style::default().fg(TEXT_DIM)),
    ])
}

// ── Now Playing bar ────────────────────────────────────────────────────────

pub fn draw_now_playing_bar(frame: &mut Frame, area: Rect, app: &App) {
    if let Some(ref np) = app.now_playing {
        draw_now_playing_active(frame, area, np);
    } else {
        draw_now_playing_empty(frame, area);
    }
}

fn draw_now_playing_active(frame: &mut Frame, area: Rect, np: &NowPlaying) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(4)])
        .split(area);

    // ── Custom Unicode progress bar ━━━━━━━●─────────────────
    let progress = if np.duration_secs > 0 {
        (np.position_secs as f64 / np.duration_secs as f64).min(1.0)
    } else {
        0.0
    };
    let pos_str = format_duration(np.position_secs);
    let dur_str = format_duration(np.duration_secs);
    let time_label = format!(" {} / {} ", pos_str, dur_str);
    let bar_width = chunks[0].width.saturating_sub(time_label.len() as u16 + 2) as usize;
    let fine_pos = progress * bar_width as f64;
    let filled = fine_pos as usize;
    let remaining = bar_width.saturating_sub(filled + 1);
    // Half-block precision for smoother progress
    let sub_blocks = ["─", "╿", "╸", "━"];
    let frac = ((fine_pos - filled as f64) * sub_blocks.len() as f64) as usize;
    let knob = sub_blocks[frac.min(sub_blocks.len() - 1)];

    let progress_line = Line::from(vec![
        Span::styled(" ", Style::default()),
        Span::styled("━".repeat(filled), Style::default().fg(BRAND)),
        Span::styled("●", Style::default().fg(TEXT).bold()),
        Span::styled(knob, Style::default().fg(BRAND_DIM)),
        Span::styled(
            "─".repeat(remaining.saturating_sub(1)),
            Style::default().fg(DIM),
        ),
        Span::styled(time_label, Style::default().fg(TEXT_DIM)),
    ]);
    frame.render_widget(Paragraph::new(progress_line), chunks[0]);

    // ── 2-line info block ──────────────────────────────────────
    let status_icon = if np.paused { "⏸" } else { "▶" };
    let title_max = area.width.saturating_sub(12) as usize;
    let title: String = np.video.title.chars().take(title_max.max(20)).collect();
    let channel = np.video.channel.as_deref().unwrap_or("Unknown");
    let repeat_label = np.repeat.label();
    let shuffle_str = if np.shuffle { "  🔀" } else { "" };
    let sleep_str = np
        .sleep_deadline
        .map(|d| format!("  😴{}", d.format("%H:%M")))
        .unwrap_or_default();

    // Line 1: status + title + badges
    let line1 = Line::from(vec![
        Span::styled(
            format!("  {} ", status_icon),
            Style::default().fg(BRAND).bold(),
        ),
        Span::styled(&title, Style::default().fg(TEXT).bold()),
        if np.is_fav {
            Span::styled(" ❤", Style::default().fg(LOVE))
        } else {
            Span::raw("")
        },
        if np.in_queue {
            Span::styled(" 📋", Style::default().fg(ACCENT))
        } else {
            Span::raw("")
        },
    ]);

    // Line 2: channel · volume · speed · eq · repeat · shuffle · sleep
    let eq_str = if np.eq_preset != "flat" {
        format!("🎛️{}", np.eq_preset)
    } else {
        "🎛️flat".to_string()
    };
    let line2 = Line::from(vec![
        Span::styled(format!("  {}", channel), Style::default().fg(TEXT_DIM)),
        Span::styled("  ·  ", Style::default().fg(DIM)),
        Span::styled(format!("🔊 {}%", np.volume), Style::default().fg(TEXT_DIM)),
        Span::styled("  ·  ", Style::default().fg(DIM)),
        Span::styled(
            format!("{}x", np.speed),
            Style::default().fg(if (np.speed - 1.0).abs() > 0.01 {
                WARN
            } else {
                TEXT_DIM
            }),
        ),
        Span::styled("  ·  ", Style::default().fg(DIM)),
        Span::styled(
            &eq_str,
            Style::default().fg(if np.eq_preset != "flat" {
                ACCENT
            } else {
                TEXT_DIM
            }),
        ),
        Span::styled("  ·  ", Style::default().fg(DIM)),
        Span::styled(
            format!("{}{}", repeat_label, shuffle_str),
            Style::default().fg(TEXT_DIM),
        ),
        Span::styled(sleep_str, Style::default().fg(WARN)),
    ]);

    let info = Paragraph::new(vec![line1, line2]).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(BRAND_DIM)),
    );
    frame.render_widget(info, chunks[1]);
}

fn draw_now_playing_empty(frame: &mut Frame, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(4)])
        .split(area);

    // Empty progress bar
    let bar_width = chunks[0].width.saturating_sub(2) as usize;
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ", Style::default()),
            Span::styled("─".repeat(bar_width), Style::default().fg(DIM)),
        ])),
        chunks[0],
    );

    // Empty info
    let empty = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("  ⏹ ", Style::default().fg(DIM)),
            Span::styled("No track playing", Style::default().fg(TEXT_DIM)),
        ]),
        Line::from(vec![
            Span::styled("  Use ", Style::default().fg(DIM)),
            Span::styled("aux search", Style::default().fg(ACCENT)),
            Span::styled(" or ", Style::default().fg(DIM)),
            Span::styled("aux play <url>", Style::default().fg(ACCENT)),
            Span::styled(" to start", Style::default().fg(DIM)),
        ]),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(DIM)),
    );
    frame.render_widget(empty, chunks[1]);
}

// ── Keybind bar ────────────────────────────────────────────────────────────

fn draw_keybind_bar(frame: &mut Frame, area: Rect, app: &App) {
    // ── Line 1: status + panel-specific key badges ─────────────
    let mut line1: Vec<Span> = Vec::new();

    if let Some(ref msg) = app.status_message {
        line1.push(Span::styled(
            format!(" ✦ {} ", msg),
            Style::default().fg(WARN),
        ));
        line1.push(Span::styled("│ ", Style::default().fg(DIM)));
    } else {
        line1.push(Span::styled(" ", Style::default()));
    }

    let panel_keys: Vec<(&str, &str)> = match app.panel {
        Panel::Search => vec![
            ("Enter", "search"),
            ("Tab", "panels"),
            ("?", "help"),
            ("q", "quit"),
        ],
        Panel::Results => vec![
            ("Enter", "play"),
            ("↑↓", "nav"),
            ("←→", "page"),
            ("a", "queue"),
            ("f", "fav"),
            ("l", "playlist"),
            ("Tab", "panel"),
        ],
        Panel::Lyrics => vec![
            ("⇧↑↓", "scroll"),
            ("0", "auto"),
            ("Tab", "panel"),
            ("Esc", "back"),
        ],
        Panel::Queue => vec![
            ("Enter", "play"),
            ("↑↓", "nav"),
            ("d", "remove"),
            ("l", "playlist"),
            ("Tab", "panel"),
        ],
        Panel::Favorites => vec![
            ("Enter", "play"),
            ("↑↓", "nav"),
            ("d", "unfav"),
            ("l", "playlist"),
            ("Tab", "panel"),
        ],
        Panel::History => vec![
            ("Enter", "replay"),
            ("↑↓", "nav"),
            ("l", "playlist"),
            ("Tab", "panel"),
        ],
        Panel::Playlists => {
            if app.playlist_items_view.is_some() {
                vec![
                    ("Enter", "play"),
                    ("↑↓", "nav"),
                    ("d", "remove"),
                    ("Esc", "back"),
                ]
            } else {
                vec![
                    ("Enter", "view"),
                    ("↑↓", "nav"),
                    ("n", "new"),
                    ("d", "del"),
                    ("p", "play"),
                ]
            }
        }
        Panel::Chat => vec![
            ("Enter", "send"),
            ("↑↓", "scroll"),
            ("Esc", "back"),
            ("Tab", "panel"),
        ],
        Panel::Help => vec![("↑↓", "scroll"), ("Esc", "back")],
    };

    for (key, action) in &panel_keys {
        line1.push(Span::styled(
            format!(" {} ", key),
            Style::default().fg(TEXT).bg(KEY_BG),
        ));
        line1.push(Span::styled(
            format!(" {} ", action),
            Style::default().fg(TEXT_DIM),
        ));
    }

    // ── Line 2: playback key badges ────────────────────────────
    let line2 = if app.now_playing.is_some() {
        let play_keys: Vec<(&str, &str)> = vec![
            ("Space", "⏯"),
            ("←→", "seek"),
            ("+/-", "vol"),
            ("]/[", "spd"),
            ("e", "eq"),
            ("r", "repeat"),
            ("n", "next"),
            ("f", "fav"),
            ("S", "stop"),
        ];
        let mut spans: Vec<Span> = vec![Span::styled(" ♪ ", Style::default().fg(BRAND))];
        for (key, action) in &play_keys {
            spans.push(Span::styled(
                format!(" {} ", key),
                Style::default().fg(TEXT).bg(KEY_BG),
            ));
            spans.push(Span::styled(
                format!(" {} ", action),
                Style::default().fg(TEXT_DIM),
            ));
        }
        Line::from(spans)
    } else {
        Line::from(Span::raw(""))
    };

    let bar = Paragraph::new(vec![Line::from(line1), line2]);
    frame.render_widget(bar, area);
}
