use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{
        Block, BorderType, Borders, Gauge, List, ListItem, Paragraph, Wrap,
    },
    Frame,
};

use crate::youtube::types::format_duration;
use super::app::{App, AppMode};

const BRAND_COLOR: Color = Color::Rgb(255, 107, 107);
const ACCENT_COLOR: Color = Color::Rgb(78, 205, 196);
const DIM_COLOR: Color = Color::Rgb(100, 100, 120);
const BG_COLOR: Color = Color::Rgb(20, 20, 30);

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Main layout: header + body + footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(5),    // Body
            Constraint::Length(3), // Now Playing bar
            Constraint::Length(1), // Status bar
        ])
        .split(area);

    draw_header(frame, chunks[0]);
    draw_body(frame, chunks[1], app);
    draw_now_playing(frame, chunks[2], app);
    draw_status_bar(frame, chunks[3], app);
}

fn draw_header(frame: &mut Frame, area: Rect) {
    let header = Paragraph::new(Line::from(vec![
        Span::styled("  🎵 ", Style::default()),
        Span::styled("duet", Style::default().fg(BRAND_COLOR).bold()),
        Span::styled(
            "  — CLI YouTube Player with AI Companion",
            Style::default().fg(DIM_COLOR),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(DIM_COLOR)),
    );

    frame.render_widget(header, area);
}

fn draw_body(frame: &mut Frame, area: Rect, app: &App) {
    match app.mode {
        AppMode::Search => draw_search(frame, area, app),
        AppMode::Results => draw_results(frame, area, app),
        AppMode::Playing => draw_playing(frame, area, app),
        AppMode::Help => draw_help(frame, area),
        _ => draw_search(frame, area, app),
    }
}

fn draw_search(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
        ])
        .split(area);

    // Search input
    let input = Paragraph::new(Line::from(vec![
        Span::styled("🔍 ", Style::default()),
        Span::styled(&app.search_input, Style::default().fg(Color::White)),
        Span::styled("▌", Style::default().fg(ACCENT_COLOR)),
    ]))
    .block(
        Block::default()
            .title(" Search YouTube ")
            .title_style(Style::default().fg(ACCENT_COLOR).bold())
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(ACCENT_COLOR)),
    );

    frame.render_widget(input, chunks[0]);

    // Help text
    let help = Paragraph::new(vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Type to search, ", Style::default().fg(DIM_COLOR)),
            Span::styled("Enter", Style::default().fg(ACCENT_COLOR)),
            Span::styled(" to search, ", Style::default().fg(DIM_COLOR)),
            Span::styled("?", Style::default().fg(ACCENT_COLOR)),
            Span::styled(" for help, ", Style::default().fg(DIM_COLOR)),
            Span::styled("Esc", Style::default().fg(ACCENT_COLOR)),
            Span::styled(" to quit", Style::default().fg(DIM_COLOR)),
        ]),
    ]);

    frame.render_widget(help, chunks[1]);
}

fn draw_results(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .search_results
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let duration = v
                .duration
                .map(|d| format_duration(d as u64))
                .unwrap_or_else(|| "LIVE".to_string());
            let channel = v.channel.as_deref().unwrap_or("Unknown");

            let style = if i == app.selected_index {
                Style::default().fg(ACCENT_COLOR).bold()
            } else {
                Style::default().fg(Color::White)
            };

            let prefix = if i == app.selected_index { "▸ " } else { "  " };

            ListItem::new(Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(&v.title, style),
                Span::styled(
                    format!("  {} · {}", channel, duration),
                    Style::default().fg(DIM_COLOR),
                ),
            ]))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(format!(" Search Results ({}) ", app.search_results.len()))
            .title_style(Style::default().fg(ACCENT_COLOR).bold())
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(DIM_COLOR)),
    );

    frame.render_widget(list, area);
}

fn draw_playing(frame: &mut Frame, area: Rect, app: &App) {
    if let Some(ref np) = app.now_playing {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(2)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(2),
                Constraint::Min(3),
            ])
            .split(area);

        // Title
        let fav_icon = if np.is_fav { " ❤️" } else { "" };
        let status_icon = if np.paused { "⏸" } else { "▶" };
        let title = Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {} ", status_icon),
                Style::default().fg(ACCENT_COLOR),
            ),
            Span::styled(
                &np.video.title,
                Style::default().fg(Color::White).bold(),
            ),
            Span::styled(fav_icon, Style::default()),
        ]));

        frame.render_widget(title, chunks[0]);

        // Channel + volume
        let channel = np.video.channel.as_deref().unwrap_or("Unknown");
        let info = Paragraph::new(Line::from(vec![
            Span::styled(
                format!("  🎵 {} ", channel),
                Style::default().fg(DIM_COLOR),
            ),
            Span::styled(
                format!("  🔊 {}%", np.volume),
                Style::default().fg(DIM_COLOR),
            ),
        ]));

        frame.render_widget(info, chunks[1]);

        // Keybinds
        let keys = Paragraph::new(vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  space", Style::default().fg(ACCENT_COLOR)),
                Span::styled(" pause  ", Style::default().fg(DIM_COLOR)),
                Span::styled("←/→", Style::default().fg(ACCENT_COLOR)),
                Span::styled(" seek  ", Style::default().fg(DIM_COLOR)),
                Span::styled("↑/↓", Style::default().fg(ACCENT_COLOR)),
                Span::styled(" vol  ", Style::default().fg(DIM_COLOR)),
                Span::styled("f", Style::default().fg(ACCENT_COLOR)),
                Span::styled(" fav  ", Style::default().fg(DIM_COLOR)),
                Span::styled("s", Style::default().fg(ACCENT_COLOR)),
                Span::styled(" search  ", Style::default().fg(DIM_COLOR)),
                Span::styled("q", Style::default().fg(ACCENT_COLOR)),
                Span::styled(" quit", Style::default().fg(DIM_COLOR)),
            ]),
        ]);

        frame.render_widget(keys, chunks[2]);
    }
}

fn draw_now_playing(frame: &mut Frame, area: Rect, app: &App) {
    if let Some(ref np) = app.now_playing {
        let progress = if np.duration_secs > 0 {
            (np.position_secs as f64 / np.duration_secs as f64).min(1.0)
        } else {
            0.0
        };

        let pos_str = format_duration(np.position_secs);
        let dur_str = format_duration(np.duration_secs);
        let status = if np.paused { "⏸" } else { "▶" };

        let gauge = Gauge::default()
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(DIM_COLOR)),
            )
            .gauge_style(Style::default().fg(BRAND_COLOR))
            .ratio(progress)
            .label(format!(
                " {} {} — {} / {} ",
                status,
                np.video.title.chars().take(40).collect::<String>(),
                pos_str,
                dur_str
            ));

        frame.render_widget(gauge, area);
    } else {
        let empty = Paragraph::new(Line::from(vec![Span::styled(
            "  No track playing",
            Style::default().fg(DIM_COLOR),
        )]))
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(DIM_COLOR)),
        );

        frame.render_widget(empty, area);
    }
}

fn draw_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let msg = app
        .status_message
        .as_deref()
        .unwrap_or("duet v0.1.0 — Type to search, ? for help");

    let status = Paragraph::new(Line::from(vec![
        Span::styled(" ", Style::default()),
        Span::styled(msg, Style::default().fg(DIM_COLOR)),
    ]));

    frame.render_widget(status, area);
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let help_text = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Keybindings", Style::default().fg(BRAND_COLOR).bold()),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  /         ", Style::default().fg(ACCENT_COLOR)),
            Span::styled("Search", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  Enter     ", Style::default().fg(ACCENT_COLOR)),
            Span::styled("Select / Confirm", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  Space     ", Style::default().fg(ACCENT_COLOR)),
            Span::styled("Pause / Resume", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  ← / →     ", Style::default().fg(ACCENT_COLOR)),
            Span::styled("Seek ±10 seconds", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  ↑ / ↓     ", Style::default().fg(ACCENT_COLOR)),
            Span::styled("Volume ±5", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  f         ", Style::default().fg(ACCENT_COLOR)),
            Span::styled("Toggle favorite", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  a         ", Style::default().fg(ACCENT_COLOR)),
            Span::styled("Add to queue", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  c         ", Style::default().fg(ACCENT_COLOR)),
            Span::styled("Chat with AI", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  h         ", Style::default().fg(ACCENT_COLOR)),
            Span::styled("History", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  ?         ", Style::default().fg(ACCENT_COLOR)),
            Span::styled("Help (this screen)", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  Esc / q   ", Style::default().fg(ACCENT_COLOR)),
            Span::styled("Back / Quit", Style::default().fg(Color::White)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Press any key to go back", Style::default().fg(DIM_COLOR)),
        ]),
    ];

    let help = Paragraph::new(help_text).block(
        Block::default()
            .title(" Help ")
            .title_style(Style::default().fg(BRAND_COLOR).bold())
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(DIM_COLOR)),
    );

    frame.render_widget(help, area);
}
