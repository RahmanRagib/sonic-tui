//! The interface: a search box, the songs it found, and what is on the turntable.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, LineGauge, List, ListItem, ListState, Paragraph};

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
        Constraint::Length(7),
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
        " Search YouTube Music (press esc) "
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

    // The state of the player, in words. There is no icon beside the title of the
    // song: the panel says what is going on instead.
    let state = match playing.status {
        Status::Idle => "nothing playing",
        Status::Loading => "loading the stream",
        Status::Playing => "",
        Status::Paused => "paused",
        Status::Failed => "failed",
    };

    let headline = if playing.title.is_empty() {
        if playing.status == Status::Failed {
            playing.error.clone().unwrap_or_else(|| state.to_string())
        } else {
            state.to_string()
        }
    } else {
        let album = if playing.album.is_empty() {
            String::new()
        } else {
            format!(" · {}", playing.album)
        };
        format!("{} — {}{album}", playing.title, playing.artist)
    };

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
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    // The bar: one unbroken line, filling from the left at the speed of the music.
    let gauge = LineGauge::default()
        .filled_symbol("━")
        .unfilled_symbol("━")
        .filled_style(Style::default().fg(THEME.fill))
        .unfilled_style(Style::default().fg(THEME.track))
        .ratio(ratio)
        .label("");
    frame.render_widget(gauge, rows[1]);

    // The gauge keeps the first cell of the line for its label, so the line is
    // started by hand, and the tip of the bar is the one that shines.
    let tip = rows[1].x + 1 + (ratio * f64::from(rows[1].width - 1)).floor() as u16;
    let shine = if blink(app.tick) {
        THEME.head
    } else {
        THEME.accent
    };
    let buffer = frame.buffer_mut();
    buffer[(rows[1].x, rows[1].y)]
        .set_symbol("━")
        .set_style(Style::default().fg(THEME.track));
    if tip < rows[1].x + rows[1].width {
        buffer[(tip, rows[1].y)].set_fg(shine);
    }

    // The panel says what is going on, in words, where an icon used to be.
    let mut title = vec![Span::styled(
        " Now playing ",
        Style::default().fg(THEME.title),
    )];
    if !state.is_empty() {
        title.push(Span::styled(
            format!("· {state} "),
            Style::default().fg(THEME.accent_title),
        ));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(THEME.border))
        .style(Style::default().bg(THEME.surface))
        .title(Line::from(title));

    frame.render_widget(
        Paragraph::new(Span::styled(headline, Style::default().fg(THEME.text))).block(block),
        rows[0],
    );

    let mut footer = vec![Span::styled(running_time, Style::default().fg(THEME.dim))];
    if playing.status == Status::Failed && !playing.title.is_empty() {
        let error = playing.error.clone().unwrap_or_default();
        footer.push(Span::styled(
            format!("  {error}"),
            Style::default().fg(THEME.accent_title),
        ));
    }
    footer.push(Span::styled(
        format!("    {loudness}"),
        Style::default().fg(THEME.dim),
    ));
    frame.render_widget(Paragraph::new(Line::from(footer)), rows[2]);

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
    frame.render_widget(title, rows[3]);

    let hints = Line::from(Span::styled(
        "enter play  space pause  n next  p prev  -/+ volume  s stop  / search  q quit",
        Style::default().fg(THEME.accent),
    ));
    frame.render_widget(Paragraph::new(hints), rows[4]);
}

/// Whether the tip of the bar is at the bright half of its blink.
pub fn blink(tick: u64) -> bool {
    tick % 4 < 2
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
    use ratatui::style::Color;
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

    /// The row of the interface that holds the given symbol.
    fn row_with(buffer: &ratatui::buffer::Buffer, symbol: &str) -> Vec<ratatui::buffer::Cell> {
        for y in 0..buffer.area.height {
            let row: String = (0..buffer.area.width)
                .map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()).unwrap_or(" "))
                .collect();
            if row.contains(symbol) {
                return (0..buffer.area.width)
                    .filter_map(|x| buffer.cell((x, y)).cloned())
                    .collect();
            }
        }
        panic!("there is no row with “{symbol}”");
    }

    #[test]
    fn the_bar_is_one_unbroken_line() {
        let buffer = render(&view(42));
        let line: String = row_with(&buffer, "━")
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(!line.contains(' '), "{line}");
        assert_eq!(line.chars().count(), 90);
    }

    #[test]
    fn the_bar_fills_from_the_left() {
        let buffer = render(&view(42));
        let line = row_with(&buffer, "━");
        let moss = line.iter().filter(|cell| cell.fg == THEME.fill).count();
        let track = line.iter().filter(|cell| cell.fg == THEME.track).count();
        assert!(moss > 0, "the bar is empty");
        assert!(track > 0, "the bar has no track");
        // The filled part comes first.
        let first_moss = line
            .iter()
            .position(|cell| cell.fg == THEME.fill)
            .expect("the filled part of the bar");
        let last_track = line
            .iter()
            .rposition(|cell| cell.fg == THEME.track)
            .expect("the track of the bar");
        assert!(first_moss < last_track);
    }

    #[test]
    fn the_bar_grows_with_the_music() {
        let early = render(&view(42));
        let later = render(&view(84));
        let moss = |buffer: &ratatui::buffer::Buffer| {
            row_with(buffer, "━")
                .iter()
                .filter(|cell| cell.fg == THEME.fill)
                .count()
        };
        assert!(moss(&later) > moss(&early));
    }

    #[test]
    fn the_tip_of_the_bar_shines() {
        let buffer = render(&view(42));
        let line = row_with(&buffer, "━");
        let shining = line
            .iter()
            .filter(|cell| cell.fg == THEME.head || cell.fg == THEME.accent)
            .count();
        assert_eq!(shining, 1);
    }

    #[test]
    fn the_keys_are_green_under_a_mandarin_title() {
        let buffer = render(&view(42));
        let keys = row_of(&buffer, "enter play");
        assert_eq!(keys[0].fg, THEME.accent);
        assert_eq!(keys[0].fg, Color::Rgb(0x4a, 0xde, 0x80));

        let title = row_of(&buffer, "Keys");
        let mandarin = title
            .iter()
            .find(|cell| cell.symbol() == "K")
            .expect("the title of the keys");
        assert_eq!(mandarin.fg, THEME.accent_title);
    }

    #[test]
    fn the_search_box_says_how_to_leave_it() {
        let mut view = view(42);
        view.editing = true;
        let buffer = render(&view);
        let row: String = row_of(&buffer, "Search YouTube Music")
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(row.contains("(press esc)"), "{row}");
    }

    #[test]
    fn the_title_carries_no_icon() {
        let buffer = render(&view(42));
        let now = row_of(&buffer, "One More Time (Radio Edit) — Daft Punk");
        assert!(!now.iter().any(|cell| cell.symbol() == "▶"));
        assert!(!now.iter().any(|cell| cell.symbol() == "⏸"));
    }

    #[test]
    fn the_state_is_written_in_the_title_of_the_panel() {
        let buffer = render(&view(42));
        let panel = row_of(&buffer, "Now playing");
        let state: String = panel.iter().map(|cell| cell.symbol()).collect::<String>();
        assert!(state.contains("Now playing"), "{state}");
        assert!(!state.contains("paused"), "{state}");
    }
}
