use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Gauge, List, ListItem, Paragraph, Tabs,
    },
    Frame,
};

use crate::youtube::types::format_duration;
use super::app::{App, NowPlaying, Panel};

// ── Color Palette (Spotify-inspired dark) ──────────────────────────────────
const BRAND:    Color = Color::Rgb(29, 185, 84);   // Spotify green
const ACCENT:   Color = Color::Rgb(78, 205, 196);  // Teal highlight
const WARN:     Color = Color::Rgb(255, 200, 60);  // Amber for repeat/sleep
const DIM:      Color = Color::Rgb(100, 100, 120);
const TEXT:     Color = Color::White;
const SELECTED: Color = Color::Rgb(29, 185, 84);

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header + tabs
            Constraint::Min(5),    // Body panel
            Constraint::Length(4), // Now playing bar
            Constraint::Length(1), // Keybind bar
        ])
        .split(area);

    draw_header(frame, chunks[0], app);
    draw_body(frame, chunks[1], app);
    draw_now_playing_bar(frame, chunks[2], app);
    draw_keybind_bar(frame, chunks[3], app);
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
        Span::styled("duet", Style::default().fg(BRAND).bold()),
    ]))
    .block(
        Block::default()
            .borders(Borders::BOTTOM | Borders::RIGHT)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(DIM)),
    );
    frame.render_widget(brand, layout[0]);

    // Tabs
    let tab_titles = vec!["Search", "Results", "Queue", "History", "Chat", "Help"];
    let selected = match app.panel {
        Panel::Search => 0,
        Panel::Results => 1,
        Panel::Queue => 2,
        Panel::History => 3,
        Panel::Chat => 4,
        Panel::Help => 5,
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
        .highlight_style(Style::default().fg(BRAND).bold().add_modifier(Modifier::UNDERLINED));

    frame.render_widget(tabs, layout[1]);
}

// ── Body dispatch ──────────────────────────────────────────────────────────

fn draw_body(frame: &mut Frame, area: Rect, app: &App) {
    match app.panel {
        Panel::Search => draw_search(frame, area, app),
        Panel::Results => draw_results(frame, area, app),
        Panel::Queue => draw_queue(frame, area, app),
        Panel::History => draw_history(frame, area, app),
        Panel::Chat => draw_chat(frame, area, app),
        Panel::Help => draw_help(frame, area),
    }
}

// ── Search panel ───────────────────────────────────────────────────────────

fn draw_search(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Length(3), Constraint::Min(3)])
        .split(area);

    // Show history position in title when navigating
    let title = if let Some(idx) = app.search_history_index {
        format!(
            " Search YouTube  ·  history {}/{} (↑↓ to navigate) ",
            idx + 1,
            app.search_history.len()
        )
    } else if !app.search_history.is_empty() {
        format!(
            " Search YouTube  ·  {} saved {} ",
            app.search_history.len(),
            if app.search_history.len() == 1 { "query" } else { "queries" }
        )
    } else {
        " Search YouTube ".to_string()
    };

    let border_color = if app.search_history_index.is_some() { WARN } else { ACCENT };

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
    let hint_spans: Vec<Span> = if !app.search_history.is_empty() && app.search_history_index.is_none() {
        let preview = app.search_history.first().map(|s| {
            if s.len() > 30 { format!("{}…", &s[..30]) } else { s.clone() }
        }).unwrap_or_default();
        vec![
            Span::styled("  Last: ", Style::default().fg(DIM)),
            Span::styled(preview, Style::default().fg(WARN)),
            Span::styled("  ↑/↓ recall history  ", Style::default().fg(DIM)),
            Span::styled("Enter", Style::default().fg(ACCENT)),
            Span::styled(" search  ", Style::default().fg(DIM)),
            Span::styled("Tab", Style::default().fg(ACCENT)),
            Span::styled(" panels  ", Style::default().fg(DIM)),
            Span::styled("?", Style::default().fg(ACCENT)),
            Span::styled(" help", Style::default().fg(DIM)),
        ]
    } else {
        vec![
            Span::styled("  Type to search  ", Style::default().fg(DIM)),
            Span::styled("Enter", Style::default().fg(ACCENT)),
            Span::styled(" to search  ", Style::default().fg(DIM)),
            Span::styled("Tab", Style::default().fg(ACCENT)),
            Span::styled(" switch panels  ", Style::default().fg(DIM)),
            Span::styled("?", Style::default().fg(ACCENT)),
            Span::styled(" help  ", Style::default().fg(DIM)),
            Span::styled("q", Style::default().fg(ACCENT)),
            Span::styled(" quit", Style::default().fg(DIM)),
        ]
    };

    let hint = Paragraph::new(vec![
        Line::from(""),
        Line::from(hint_spans),
    ]);
    frame.render_widget(hint, chunks[1]);
}

// ── Results panel ─────────────────────────────────────────────────────────

fn draw_results(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
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
            let style = if selected {
                Style::default().fg(SELECTED).bold()
            } else {
                Style::default().fg(TEXT)
            };
            let prefix = if selected { "▸ " } else { "  " };

            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(prefix, style),
                    Span::styled(format!("{}. ", global_idx), Style::default().fg(DIM)),
                    Span::styled(&v.title, style),
                ]),
                Line::from(vec![
                    Span::styled("    ", Style::default()),
                    Span::styled(
                        format!("{}  ·  {}", channel, duration),
                        Style::default().fg(DIM),
                    ),
                ]),
            ])
        })
        .collect();

    let title = format!(
        " Results ({})  ·  page {}/{} ",
        app.all_search_results.len(),
        app.search_page + 1,
        app.search_total_pages(),
    );
    let list = List::new(items).block(
        Block::default()
            .title(title)
            .title_style(Style::default().fg(ACCENT).bold())
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(DIM)),
    );
    frame.render_widget(list, area);
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

    let items: Vec<ListItem> = app
        .queue_items
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let selected = i == app.selected_index;
            let style = if selected {
                Style::default().fg(SELECTED).bold()
            } else {
                Style::default().fg(TEXT)
            };
            let prefix = if selected { "▸ " } else { "  " };
            let dur = e
                .duration_secs
                .map(|d| format_duration(d as u64))
                .unwrap_or_else(|| "??:??".to_string());
            ListItem::new(Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(&e.title, style),
                Span::styled(format!("  [{}]", dur), Style::default().fg(DIM)),
            ]))
        })
        .collect();

    let title = format!(" Queue ({}) ", app.queue_items.len());
    let list = List::new(items).block(
        Block::default()
            .title(title)
            .title_style(Style::default().fg(ACCENT).bold())
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(DIM)),
    );
    frame.render_widget(list, area);
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

    let items: Vec<ListItem> = app
        .history_items
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let selected = i == app.selected_index;
            let style = if selected {
                Style::default().fg(SELECTED).bold()
            } else {
                Style::default().fg(TEXT)
            };
            let prefix = if selected { "▸ " } else { "  " };
            let ch = e.channel.as_deref().unwrap_or("Unknown");
            let when = e.played_at.split('T').next().unwrap_or(&e.played_at);
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(prefix, style),
                    Span::styled(&e.title, style),
                ]),
                Line::from(vec![
                    Span::styled("    ", Style::default()),
                    Span::styled(
                        format!("{}  ·  {}", ch, when),
                        Style::default().fg(DIM),
                    ),
                ]),
            ])
        })
        .collect();

    let title = format!(" History ({}) ", app.history_items.len());
    let list = List::new(items).block(
        Block::default()
            .title(title)
            .title_style(Style::default().fg(ACCENT).bold())
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(DIM)),
    );
    frame.render_widget(list, area);
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
                    if msg.role == "user" { "You" } else { "Duet" },
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

    let title = format!(
        " Chat ({} messages) ",
        app.chat_messages.len()
    );
    let messages_widget = Paragraph::new(lines)
        .scroll((scroll, 0))
        .block(
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
        Span::styled("💬 ", Style::default()),
        Span::styled(&app.chat_input, Style::default().fg(TEXT).bold()),
        Span::styled("▌", Style::default().fg(ACCENT)),
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

fn draw_help(frame: &mut Frame, area: Rect) {
    let lines = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "  Keybindings",
            Style::default().fg(BRAND).bold(),
        )]),
        Line::from(""),
        help_row("/", "New search"),
        help_row("Enter", "Play selected / confirm"),
        help_row("Tab", "Cycle panels (Search → Results → Queue → History → Help)"),
        help_row("↑ ↓ / j k", "Navigate list"),
        help_row("← / →", "Page prev / next  (Results panel)"),
        help_row("Esc / q", "Back / Quit"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  Playback (any panel while playing)",
            Style::default().fg(ACCENT).bold(),
        )]),
        Line::from(""),
        help_row("Space", "Pause / Resume"),
        help_row("← / →", "Seek ±10s  (Search/Help panel)"),
        help_row("Shift + ← / →", "Seek ±60s  (any panel)"),
        help_row("↑ / ↓", "Volume ±5%  (Search/Help panel)"),
        help_row("+ / -  or  =", "Volume ±5%  (any panel)"),
        help_row("] / [", "Speed up / down"),
        help_row("n", "Skip to next track"),
        help_row("p", "Restart / prev track"),
        help_row("r", "Cycle repeat  (off → one → all)"),
        help_row("z", "Toggle shuffle"),
        help_row("t", "Toggle 30-min sleep timer  (shows in bar)"),
        help_row("f", "Toggle favorite"),
        help_row("a", "Add current track to queue"),
        help_row("d", "Remove selected item from queue  (Queue panel only)"),
        help_row("S", "Stop playback"),
        help_row("c", "Chat hint  (use duet chat in terminal)"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  Chat Panel",
            Style::default().fg(ACCENT).bold(),
        )]),
        Line::from(""),
        help_row("Enter", "Send message to AI"),
        help_row("↑ / ↓", "Scroll chat history"),
        help_row("Esc", "Back to Search"),
        Line::from(""),
        help_row("?", "This help screen"),
    ];

    let help = Paragraph::new(lines).block(
        Block::default()
            .title(" Help ")
            .title_style(Style::default().fg(BRAND).bold())
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(DIM)),
    );
    frame.render_widget(help, area);
}

fn help_row<'a>(key: &'a str, desc: &'a str) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("  {:<18}", key), Style::default().fg(ACCENT)),
        Span::styled(desc, Style::default().fg(TEXT)),
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
        .constraints([Constraint::Length(1), Constraint::Length(3)])
        .split(area);

    // Progress bar
    let progress = if np.duration_secs > 0 {
        (np.position_secs as f64 / np.duration_secs as f64).min(1.0)
    } else {
        0.0
    };
    let pos_str = format_duration(np.position_secs);
    let dur_str = format_duration(np.duration_secs);

    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(BRAND).bg(Color::Rgb(30, 30, 30)))
        .ratio(progress)
        .label(format!("{} / {}", pos_str, dur_str));
    frame.render_widget(gauge, chunks[0]);

    // Info line
    let status = if np.paused { "⏸" } else { "▶" };
    let fav   = if np.is_fav  { " ❤" } else { "" };
    let queue = if np.in_queue { " 📋" } else { "" };
    let channel = np.video.channel.as_deref().unwrap_or("Unknown");
    let repeat_label = np.repeat.label();
    let shuffle_label = if np.shuffle { " 🔀" } else { "" };

    // Truncate title to fit
    let title_max = area.width.saturating_sub(40) as usize;
    let title: String = np
        .video
        .title
        .chars()
        .take(title_max.max(20))
        .collect();

    // Sleep timer indicator
    let sleep_span = np.sleep_deadline.map(|d| {
        Span::styled(
            format!("  😴{}", d.format("%H:%M")),
            Style::default().fg(WARN),
        )
    });

    let mut info_spans = vec![
        Span::styled(format!(" {} ", status), Style::default().fg(BRAND).bold()),
        Span::styled(title, Style::default().fg(TEXT).bold()),
        Span::styled(fav,   Style::default().fg(Color::Red)),
        Span::styled(queue, Style::default().fg(ACCENT)),
        Span::styled(format!("  ·  {}", channel), Style::default().fg(DIM)),
        Span::styled(format!("  🔊{}%", np.volume), Style::default().fg(DIM)),
        Span::styled(
            format!("  {}x", np.speed),
            Style::default().fg(if (np.speed - 1.0).abs() > 0.01 { WARN } else { DIM }),
        ),
        Span::styled(
            format!("  {}{}", repeat_label, shuffle_label),
            Style::default().fg(DIM),
        ),
    ];
    if let Some(s) = sleep_span { info_spans.push(s); }

    let info = Paragraph::new(Line::from(info_spans))
    .block(
        Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(DIM)),
    );
    frame.render_widget(info, chunks[1]);
}

fn draw_now_playing_empty(frame: &mut Frame, area: Rect) {
    let empty = Paragraph::new(Line::from(vec![Span::styled(
        "  No track playing — use duet search or duet play <url>",
        Style::default().fg(DIM),
    )]))
    .block(
        Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(DIM)),
    );
    frame.render_widget(empty, area);
}

// ── Keybind bar ────────────────────────────────────────────────────────────

fn draw_keybind_bar(frame: &mut Frame, area: Rect, app: &App) {
    let playing_hint = if app.now_playing.is_some() {
        "  | Space:pause  ←→:seek  +/-:vol  ]/[:speed  r:repeat  z:shuffle  n:next  t:sleep  S:stop"
    } else {
        ""
    };

    let panel_hint = match app.panel {
        Panel::Search  => " Enter:search  Tab:panels  ?:help  q:quit",
        Panel::Results => " Enter:play  ↑↓jk:nav  ←→:page  a:queue  f:fav  Tab:panel  Esc:back",
        Panel::Queue   => " Enter:play  ↑↓jk:nav  d:remove  Tab:panel",
        Panel::History => " Enter:replay  ↑↓jk:nav  Tab:panel",
        Panel::Chat    => " Enter:send  ↑↓:scroll  Esc:back  Tab:panel",
        Panel::Help    => " Any key to go back",
    };

    let msg = format!("{}{}", panel_hint, playing_hint);

    let status_msg = app
        .status_message
        .as_deref()
        .map(|s| format!(" ✦ {}  |{}", s, msg))
        .unwrap_or_else(|| msg.to_string());

    let bar = Paragraph::new(Span::styled(status_msg, Style::default().fg(DIM)));
    frame.render_widget(bar, area);
}
