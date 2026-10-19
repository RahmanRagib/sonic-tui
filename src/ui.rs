//! The interface: a search box, the songs it found, and what is on the turntable.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use crate::playback::{NowPlaying, Status};
use crate::theme::THEME;
use crate::ytm::Track;

/// Draws the whole interface.
pub fn draw(frame: &mut Frame, app: &AppView) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(THEME.background)),
        area,
    );

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
    let (border, prompt) = if app.editing {
        (
            Style::default().fg(THEME.border_focus),
            Style::default().fg(THEME.head),
        )
    } else {
        (
            Style::default().fg(THEME.border),
            Style::default().fg(THEME.dim),
        )
    };

    let title = if app.editing {
        " Search YouTube Music "
    } else {
        " Search YouTube Music (press /) "
    };

    let mut text = app.input.to_string();
    if app.editing {
        text.push('▏');
    }

    let lines = vec![Line::from(vec![
        Span::styled("› ", prompt),
        Span::styled(text, Style::default().fg(THEME.text)),
    ])];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .style(Style::default().bg(THEME.surface))
        .title(Span::styled(title, Style::default().fg(THEME.title)));

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
            let mut spans = vec![Span::styled(
                track.title.clone(),
                Style::default().fg(THEME.text),
            )];
            if !track.artist.is_empty() {
                spans.push(Span::styled(
                    format!("  {}", track.artist),
                    Style::default().fg(THEME.dim),
                ));
            }
            if let Some(duration) = track.duration {
                spans.push(Span::styled(
                    format!("  {}", clock(duration)),
                    Style::default().fg(THEME.dim),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(THEME.border))
                .style(Style::default().bg(THEME.surface))
                .title(Span::styled(title, Style::default().fg(THEME.title))),
        )
        .highlight_style(
            Style::default()
                .fg(THEME.highlight_fg)
                .bg(THEME.highlight_bg)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(Span::styled("♪ ", Style::default().fg(THEME.head)));

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

    let mut headline = if playing.title.is_empty() {
        note.clone()
    } else {
        let album = if playing.album.is_empty() {
            String::new()
        } else {
            format!(" · {}", playing.album)
        };
        format!("{marker} {} — {}{album}", playing.title, playing.artist)
    };

    if !note.is_empty() && !playing.title.is_empty() {
        headline.push_str(&format!("  ({note})"));
    }

    let elapsed = playing.elapsed();
    let total = playing.duration.unwrap_or(0) as f64;
    let ratio = if total > 0.0 {
        (elapsed / total).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let running_time = if total > 0.0 {
        format!("{} / {}", clock(elapsed as u32), clock(total as u32))
    } else {
        format!("{} / --:--", clock(elapsed as u32))
    };
    let loudness = format!("vol {:>3}%", app.volume);

    let rows = Layout::vertical([
        Constraint::Length(4),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    // The panel, with the bar running along the bottom of it.
    let inner = rows[0].width.saturating_sub(2);
    let right = format!("{running_time}   {loudness}");
    let right_width = right.chars().count() as u16;
    let bar_width = (inner * 45 / 100)
        .max(8)
        .min(inner.saturating_sub(right_width + 1));

    let mut line = progress_bar(ratio, bar_width, app.tick);
    line.push(Span::raw(
        " ".repeat(inner.saturating_sub(bar_width + right_width) as usize),
    ));
    line.push(Span::styled(running_time, Style::default().fg(THEME.dim)));
    line.push(Span::raw("   "));
    line.push(Span::styled(loudness, Style::default().fg(THEME.dim)));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(THEME.border))
        .style(Style::default().bg(THEME.surface))
        .title(Span::styled(
            " Now playing ",
            Style::default().fg(THEME.title),
        ));

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(headline, Style::default().fg(THEME.text))),
            Line::from(line),
        ])
        .block(block),
        rows[0],
    );

    // The keys, under a mandarin title.
    let title = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(THEME.border))
        .title(Span::styled(
            " Keys ",
            Style::default()
                .fg(THEME.accent_title)
                .add_modifier(Modifier::BOLD),
        ));
    frame.render_widget(title, rows[1]);

    let hints = Line::from(Span::styled(
        "enter play  space pause  n next  p prev  -/+ volume  s stop  / search  q quit",
        Style::default().fg(THEME.accent),
    ));
    frame.render_widget(Paragraph::new(hints), rows[2]);
}

/// The bar that fills from the left as the song plays.
///
/// It grows at the speed of the music: a track of five minutes takes five minutes to fill
/// the bar. The head of the bar blinks, and the cells just behind it shine a little
/// brighter, so the bar looks alive while it moves.
fn progress_bar(ratio: f64, width: u16, tick: u64) -> Vec<Span<'static>> {
    let width = width.max(1) as usize;
    let filled = ((ratio.clamp(0.0, 1.0) * width as f64) as usize).min(width);

    let blink = if tick % 4 < 2 {
        THEME.head
    } else {
        THEME.title
    };
    let fill = Style::default().fg(THEME.fill);
    let bright = Style::default().fg(THEME.head);
    let empty = Style::default().fg(THEME.track);

    let mut spans = Vec::with_capacity(width);
    for cell in 0..width {
        let (symbol, style) = if filled >= width {
            // The song is over: the bar is full.
            ("█", if width - cell <= 3 { bright } else { fill })
        } else if cell < filled {
            ("█", if filled - cell <= 3 { bright } else { fill })
        } else if cell == filled {
            ("▌", Style::default().fg(blink))
        } else {
            ("░", empty)
        };
        spans.push(Span::styled(symbol, style));
    }
    spans
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
    /// Counts the frames, so the bar can blink.
    pub tick: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::AtomicU64;

    /// The interface as it is drawn, with a song that has been playing for a while.
    fn view(elapsed: u64) -> AppView<'static> {
        let tracks = vec![
            Track {
                id: "1".to_string(),
                title: "One More Time (Radio Edit)".to_string(),
                artist: "Daft Punk".to_string(),
                album: None,
                duration: Some(321),
            },
            Track {
                id: "2".to_string(),
                title: "One More Time (feat. Alida)".to_string(),
                artist: "Robin Schulz".to_string(),
                album: None,
                duration: Some(200),
            },
        ];
        let playing = NowPlaying {
            title: "One More Time (Radio Edit)".to_string(),
            artist: "Daft Punk".to_string(),
            album: "Discovery".to_string(),
            status: Status::Playing,
            duration: Some(321),
            played: Arc::new(AtomicU64::new(44_100 * 2 * elapsed)),
            finished: Arc::new(AtomicBool::new(false)),
            error: None,
            notes: Arc::new(Mutex::new(String::new())),
            copied: Arc::new(AtomicU64::new(0)),
            decoded: Arc::new(AtomicU64::new(0)),
        };
        AppView {
            input: "daft punk",
            editing: false,
            busy: false,
            results: Box::leak(tracks.into_boxed_slice()),
            selected: 0,
            status: "12 songs",
            now_playing: playing,
            volume: 80,
            tick: 0,
        }
    }

    /// The interface, drawn on a terminal that is not there.
    fn render(view: &AppView) -> ratatui::buffer::Buffer {
        let backend = ratatui::backend::TestBackend::new(90, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, view)).unwrap();
        terminal.backend().buffer().clone()
    }

    /// The row of the interface that holds the given text.
    fn row_of(buffer: &ratatui::buffer::Buffer, text: &str) -> Vec<ratatui::buffer::Cell> {
        for y in 0..buffer.area.height {
            let row: String = (0..buffer.area.width)
                .map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()).unwrap_or(" "))
                .collect();
            if row.contains(text) {
                return (0..buffer.area.width)
                    .filter_map(|x| buffer.cell((x, y)).cloned())
                    .collect();
            }
        }
        panic!("there is no row with “{text}”");
    }

    fn bar(ratio: f64, width: u16, tick: u64) -> String {
        progress_bar(ratio, width, tick)
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn the_bar_is_as_wide_as_it_is_told() {
        assert_eq!(bar(0.5, 20, 0).chars().count(), 20);
    }

    #[test]
    fn the_bar_fills_from_the_left() {
        let text = bar(0.25, 20, 0);
        assert!(text.starts_with("█████"), "{text}");
        assert!(text.ends_with('░'), "{text}");
    }

    #[test]
    fn the_bar_starts_empty() {
        let text = bar(0.0, 8, 0);
        assert!(text.starts_with('▌'), "{text}");
        assert!(text.ends_with('░'), "{text}");
    }

    #[test]
    fn the_bar_is_full_at_the_end_of_the_song() {
        let text = bar(1.0, 8, 0);
        assert_eq!(text, "████████");
    }

    #[test]
    fn the_head_of_the_bar_blinks() {
        let lit = progress_bar(0.5, 20, 0)[10].style;
        let dim = progress_bar(0.5, 20, 2)[10].style;
        assert_ne!(lit, dim);
    }

    #[test]
    fn the_bar_grows_with_the_music() {
        let early = render(&view(42));
        let later = render(&view(84));
        let played = |buffer: &ratatui::buffer::Buffer| {
            row_of(buffer, "█")
                .iter()
                .filter(|cell| cell.symbol() == "█")
                .count()
        };
        assert!(played(&later) > played(&early));
    }

    #[test]
    fn the_keys_are_yellow_under_a_mandarin_title() {
        let buffer = render(&view(42));
        let keys = row_of(&buffer, "enter play");
        assert_eq!(keys[0].fg, THEME.accent);

        let title = row_of(&buffer, "Keys");
        let mandarin = title
            .iter()
            .find(|cell| cell.symbol() == "K")
            .expect("the title of the keys");
        assert_eq!(mandarin.fg, THEME.accent_title);
    }
}
