//! The interface: a search box, the songs it found, and what is on the turntable.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph};

use crate::playback::{NowPlaying, Status};
use crate::ytm::Track;

/// Draws the whole interface.
pub fn draw(frame: &mut Frame, app: &AppView) {
    let area = frame.area();
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(3),
        Constraint::Length(6),
    ])
    .split(area);

    search(frame, rows[0], app);
    results(frame, rows[1], app);
    now_playing(frame, rows[2], app);
}

/// The search box.
fn search(frame: &mut Frame, area: Rect, app: &AppView) {
    let (border, title) = if app.editing {
        (Style::default().fg(Color::Cyan), " Search YouTube Music ")
    } else {
        (Style::default(), " Search YouTube Music (press /) ")
    };

    let mut text = app.input.to_string();
    if app.editing {
        text.push('▏');
    }

    let lines = vec![Line::from(vec![
        Span::styled("› ", Style::default().fg(Color::Cyan)),
        Span::raw(text),
    ])];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .title(title);

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// The songs the search found.
fn results(frame: &mut Frame, area: Rect, app: &AppView) {
    let title = if app.busy {
        " Songs (searching…) ".to_string()
    } else if app.status.is_empty() {
        " Songs ".to_string()
    } else {
        format!(" Songs · {} ", app.status)
    };

    let items: Vec<ListItem> = app
        .results
        .iter()
        .map(|track| {
            let mut spans = vec![Span::raw(track.title.clone())];
            if !track.artist.is_empty() {
                spans.push(Span::styled(
                    format!("  {}", track.artist),
                    Style::default().fg(Color::DarkGray),
                ));
            }
            if let Some(duration) = track.duration {
                spans.push(Span::styled(
                    format!("  {}", clock(duration)),
                    Style::default().fg(Color::DarkGray),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("♪ ");

    let mut state = ListState::default();
    if !app.results.is_empty() {
        state.select(Some(app.selected.min(app.results.len() - 1)));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

/// What is playing, and the keys that steer it.
fn now_playing(frame: &mut Frame, area: Rect, app: &AppView) {
    let playing = &app.now_playing;

    let (marker, note) = match playing.status {
        Status::Idle => ("·", "nothing playing".to_string()),
        Status::Loading => ("…", "loading the stream".to_string()),
        Status::Playing => ("▶", String::new()),
        Status::Paused => ("⏸", "paused".to_string()),
        Status::Failed => ("✗", playing.error.clone().unwrap_or_default()),
    };

    let headline = if playing.title.is_empty() {
        note.clone()
    } else {
        let album = if playing.album.is_empty() {
            String::new()
        } else {
            format!(" · {}", playing.album)
        };
        format!("{marker} {} — {}{album}", playing.title, playing.artist)
    };

    let elapsed = playing.elapsed();
    let total = playing.duration.unwrap_or(0) as f64;
    let ratio = if total > 0.0 {
        (elapsed / total).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let clock = if total > 0.0 {
        format!("{} / {}", clock(elapsed as u32), clock(total as u32))
    } else {
        format!("{} / --:--", clock(elapsed as u32))
    };

    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Now playing ");
    frame.render_widget(
        Paragraph::new(Line::from(Span::raw(headline))).block(block),
        rows[0],
    );

    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(Color::Cyan))
        .ratio(ratio)
        .label("");
    frame.render_widget(gauge, rows[1]);

    let mut footer = vec![Span::styled(clock, Style::default().fg(Color::DarkGray))];
    if !note.is_empty() && !playing.title.is_empty() {
        footer.push(Span::styled(
            format!("  ({note})"),
            Style::default().fg(Color::DarkGray),
        ));
    }
    footer.push(Span::styled(
        format!("    vol {:>3}%", app.volume),
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(Paragraph::new(Line::from(footer)), rows[2]);

    let hints = Line::from(Span::styled(
        "enter play  space pause  n next  p prev  -/+ volume  s stop  / search  q quit",
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(Paragraph::new(hints), rows[3]);
}

/// A running time, written as `m:ss`.
pub fn clock(seconds: u32) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// What the interface needs to draw itself.
pub struct AppView<'a> {
    pub input: &'a str,
    pub editing: bool,
    pub busy: bool,
    pub results: &'a [Track],
    pub selected: usize,
    pub status: &'a str,
    pub now_playing: NowPlaying,
    /// The loudness, in percent.
    pub volume: u8,
}
