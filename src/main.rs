//! A music streaming client for the terminal, built in the shape of Sonora: a provider that
//! knows the catalogue, an engine that plays the sound, and an interface on top of both.
//!
//! The catalogue is YouTube Music, reached as a guest: no sign-in, no account, no cookie.

mod playback;
mod theme;
mod ui;
mod ytm;

use std::io;
use std::io::IsTerminal;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;
use std::time::Instant;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use playback::{Engine, Status};
use ui::AppView;
use ytm::Track;

/// What the search thread tells the interface.
enum Message {
    Results(Vec<Track>),
    Failed(String),
}

/// How far the right and the left arrow move the sound, in seconds.
const SEEK: f64 = 2.0;

fn main() -> io::Result<()> {
    let arguments: Vec<String> = std::env::args().collect();

    match arguments.get(1).map(String::as_str) {
        Some("-h" | "--help") => {
            usage();
            return Ok(());
        }
        Some("-V" | "--version") => {
            println!("sonic-tui {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("--play") => {
            let query = arguments.get(2).cloned().unwrap_or_default();
            let index: usize = arguments
                .get(3)
                .and_then(|value| value.parse().ok())
                .unwrap_or(0);
            let seconds: u64 = arguments
                .get(4)
                .and_then(|value| value.parse().ok())
                .unwrap_or(10);
            return headless(query, index, seconds);
        }
        Some("--draw") => {
            let query = arguments.get(2).cloned().unwrap_or_default();
            return draw_once(query);
        }
        Some("--keys") => {
            let keys = arguments.get(2).cloned().unwrap_or_default();
            return scripted(&keys);
        }
        Some(other) if other.starts_with('-') => {
            eprintln!("sonic-tui: unknown option “{other}”");
            usage();
            return Ok(());
        }
        _ => {}
    }

    // Whatever is left of the line is the first search.
    let query = arguments[1..].join(" ");

    if !io::stdout().is_terminal() {
        eprintln!("sonic-tui: there is no terminal to draw in");
        return Ok(());
    }

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, query);
    ratatui::restore();
    result
}

/// Plays a script of keys through the interface, and draws what comes out.
fn scripted(keys: &str) -> io::Result<()> {
    let engine = Engine::new().map_err(io::Error::other)?;
    let (queries, messages) = searcher();
    let mut app = App {
        input: String::new(),
        editing: false,
        busy: false,
        results: Vec::new(),
        selected: 0,
        status: String::new(),
        engine: engine.clone(),
        volume: 80,
        tick: 0,
        version: 0,
    };

    let (first, rest) = match keys.split_once('\r') {
        Some((first, rest)) => (first, rest),
        None => (keys, ""),
    };

    for character in first.chars() {
        let key = KeyEvent {
            code: KeyCode::Char(character),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: ratatui::crossterm::event::KeyEventState::NONE,
        };
        app.key(key);
    }
    if !first.is_empty() {
        let key = KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: ratatui::crossterm::event::KeyEventState::NONE,
        };
        if let Action::Search = app.key(key) {
            let _ = queries.send(app.input.clone());
        }
    }

    // The search takes a moment; the interface is drawn once it has answered.
    for _ in 0..100 {
        while let Ok(message) = messages.try_recv() {
            app.receive(message);
        }
        if !app.results.is_empty() || app.status.contains("failed") {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    // The rest of the script is played against the songs that were found.
    for character in rest.chars() {
        let key = KeyEvent {
            code: match character {
                '\r' | '\n' => KeyCode::Enter,
                other => KeyCode::Char(other),
            },
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: ratatui::crossterm::event::KeyEventState::NONE,
        };
        app.key(key);
        std::thread::sleep(Duration::from_millis(50));
    }

    // A moment for the sound to start.
    std::thread::sleep(Duration::from_secs(8));

    let backend = ratatui::backend::TestBackend::new(90, 20);
    let mut terminal = ratatui::Terminal::new(backend).map_err(io::Error::other)?;
    terminal
        .draw(|frame| ui::draw(frame, &app.view()))
        .map_err(io::Error::other)?;

    let buffer = terminal.backend().buffer();
    for y in 0..buffer.area.height {
        let row: String = (0..buffer.area.width)
            .map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()).unwrap_or(" "))
            .collect();
        println!("{row}");
    }
    Ok(())
}

/// Renders one frame of the interface and writes it out.
fn draw_once(query: String) -> io::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(io::Error::other)?;
    let results = runtime
        .block_on(ytm::search(&query))
        .map_err(io::Error::other)?;

    let engine = Engine::new().map_err(io::Error::other)?;
    let app = App {
        input: query.clone(),
        editing: false,
        busy: false,
        results,
        selected: 1,
        status: "12 songs".to_string(),
        engine: engine.clone(),
        volume: 80,
        tick: 0,
        version: 0,
    };

    let backend = ratatui::backend::TestBackend::new(90, 20);
    let mut terminal = ratatui::Terminal::new(backend).map_err(io::Error::other)?;
    terminal
        .draw(|frame| ui::draw(frame, &app.view()))
        .map_err(io::Error::other)?;

    let buffer = terminal.backend().buffer();
    for y in 0..buffer.area.height {
        let row: String = (0..buffer.area.width)
            .map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()).unwrap_or(" "))
            .collect();
        println!("{row}");
    }
    Ok(())
}

/// What the command prompt is told about sonic-tui.
fn usage() {
    println!(
        "\
sonic-tui {version} — a music streaming client for the terminal

  sonic-tui                    the player, with an empty search box
  sonic-tui <words>            the player, searching for <words> right away
  sonic-tui --play <words>     search, play the first song, and report
  sonic-tui --draw <words>     draw one frame of the interface and print it
  sonic-tui --keys <keys>      play a script of keys through the interface
  sonic-tui --version          the version
  sonic-tui --help             this text

The songs come from YouTube Music, as a guest: no account, no sign-in.

Inside the player:
  /        open the search box        space   pause and resume
  enter    search, or play            n / p   the next or the previous song
  up/down  move through the songs     - / +   quieter and louder
  ←/→      move the sound             s   stop        q   quit

yt-dlp and ffmpeg have to be on the path: they fetch and decode the sound.",
        version = env!("CARGO_PKG_VERSION")
    );
}

/// Searches, plays a song, and reports what happened.
fn headless(query: String, index: usize, seconds: u64) -> io::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(io::Error::other)?;

    println!("searching for “{query}”");
    let tracks = runtime
        .block_on(ytm::search(&query))
        .map_err(io::Error::other)?;
    for (index, track) in tracks.iter().take(5).enumerate() {
        println!(
            "  {index}. {} — {} ({:?})",
            track.title, track.artist, track.duration
        );
    }
    let Some(track) = tracks.get(index).cloned() else {
        println!("nothing found");
        return Ok(());
    };

    let engine = Engine::new().map_err(io::Error::other)?;
    engine.play(track.clone());
    println!("playing {} — {}", track.title, track.artist);

    for _ in 0..(seconds * 10) {
        std::thread::sleep(Duration::from_millis(100));
        let state = engine.now_playing();
        if let Some(error) = &state.error {
            println!("failed: {error}");
            return Ok(());
        }
        if state.finished.load(std::sync::atomic::Ordering::Relaxed) {
            println!("the stream ended after {:.1}s", state.elapsed());
            println!(
                "fetched {} bytes, decoded {} bytes",
                state.copied.load(std::sync::atomic::Ordering::Relaxed),
                state.decoded.load(std::sync::atomic::Ordering::Relaxed)
            );
            let notes = state
                .notes
                .lock()
                .map(|notes| notes.clone())
                .unwrap_or_default();
            if !notes.is_empty() {
                println!("the tools said: {notes}");
            }
            return Ok(());
        }
    }

    let state = engine.now_playing();
    println!(
        "played {:.1}s of {:?} (status {:?})",
        state.elapsed(),
        state.duration,
        state.status
    );
    Ok(())
}

fn run(terminal: &mut DefaultTerminal, query: String) -> io::Result<()> {
    let engine = Engine::new().map_err(io::Error::other)?;
    let (queries, messages) = searcher();

    let mut app = App {
        input: query.clone(),
        editing: false,
        busy: !query.is_empty(),
        results: Vec::new(),
        selected: 0,
        status: String::new(),
        engine: engine.clone(),
        volume: 80,
        tick: 0,
        version: 0,
    };

    // A search given on the command line starts right away.
    if !query.is_empty() {
        let _ = queries.send(query);
    }

    let started = Instant::now();
    // What is on the screen now. The interface is only drawn when this changes,
    // so an idle player costs nothing.
    let mut shown: Option<Snapshot> = None;

    loop {
        while let Ok(message) = messages.try_recv() {
            app.receive(message);
        }
        app.check_finished();

        // The clock of the animation is the wall clock, not the number of frames,
        // so the bar and the blink keep their own time.
        app.tick = started.elapsed().as_millis() as u64 / 100;

        let snapshot = app.snapshot();
        if shown.as_ref() != Some(&snapshot) {
            terminal.draw(|frame| ui::draw(frame, &app.view()))?;
            shown = Some(snapshot);
        }

        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            match app.key(key) {
                Action::Quit => {
                    app.engine.quit();
                    return Ok(());
                }
                Action::Search => {
                    let _ = queries.send(app.input.clone());
                }
                Action::None => {}
            }
            // A key has been pressed: the picture is different now.
            shown = None;
        }
    }
}

/// What the interface shows, so that it is only drawn when it has changed.
#[derive(Debug, PartialEq)]
struct Snapshot {
    input: String,
    editing: bool,
    busy: bool,
    results: usize,
    selected: usize,
    status: String,
    volume: u8,
    title: String,
    artist: String,
    album: String,
    state: Status,
    /// The running time, in tenths of a second: the bar moves with it.
    played: u32,
    duration: Option<u32>,
    error: Option<String>,
    /// Counts the changes of the song list, so a new list is never mistaken for
    /// the old one.
    version: u64,
    /// Whether the tip of the bar is shining.
    blink: bool,
}

/// The state the interface keeps.
struct App {
    input: String,
    editing: bool,
    busy: bool,
    results: Vec<Track>,
    selected: usize,
    status: String,
    engine: Arc<Engine>,
    volume: u8,
    /// Counts the frames, so the bar can blink.
    tick: u64,
    /// Counts the changes of the song list.
    version: u64,
}

/// What a key press asks for.
enum Action {
    None,
    Quit,
    Search,
}

impl App {
    /// The interface as it is drawn.
    fn view(&self) -> AppView<'_> {
        AppView {
            input: &self.input,
            editing: self.editing,
            busy: self.busy,
            results: &self.results,
            selected: self.selected,
            status: &self.status,
            now_playing: self.engine.now_playing(),
            volume: self.volume,
            tick: self.tick,
        }
    }

    /// A picture of what the interface shows, to tell whether it has to be drawn again.
    fn snapshot(&self) -> Snapshot {
        let playing = self.engine.now_playing();
        Snapshot {
            input: self.input.clone(),
            editing: self.editing,
            busy: self.busy,
            results: self.results.len(),
            selected: self.selected,
            status: self.status.clone(),
            volume: self.volume,
            title: playing.title.clone(),
            artist: playing.artist.clone(),
            album: playing.album.clone(),
            state: playing.status,
            played: (playing.elapsed() * 10.0) as u32,
            duration: playing.duration,
            error: playing.error.clone(),
            version: self.version,
            blink: playing.status == Status::Playing && ui::blink(self.tick),
        }
    }

    /// Handles a key press.
    fn key(&mut self, key: KeyEvent) -> Action {
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
        {
            return Action::Quit;
        }

        if self.editing {
            match key.code {
                KeyCode::Esc => {
                    self.editing = false;
                }
                KeyCode::Enter => {
                    self.editing = false;
                    if !self.input.trim().is_empty() {
                        self.busy = true;
                        self.status = format!("searching for “{}”…", self.input.trim());
                        return Action::Search;
                    }
                }
                KeyCode::Backspace => {
                    self.input.pop();
                }
                KeyCode::Char(c) => self.input.push(c),
                _ => {}
            }
            return Action::None;
        }

        match key.code {
            KeyCode::Char('q') => Action::Quit,
            KeyCode::Char('/') => {
                self.editing = true;
                Action::None
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.move_by(1);
                Action::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.move_by(-1);
                Action::None
            }
            KeyCode::Right => {
                self.seek(SEEK);
                Action::None
            }
            KeyCode::Left => {
                self.seek(-SEEK);
                Action::None
            }
            KeyCode::Enter => {
                self.play_selected();
                Action::None
            }
            KeyCode::Char(' ') => {
                let status = self.engine.now_playing().status;
                match status {
                    Status::Playing => self.engine.pause(),
                    Status::Paused => self.engine.resume(),
                    _ => {}
                }
                Action::None
            }
            KeyCode::Char('n') => {
                self.step(1);
                Action::None
            }
            KeyCode::Char('p') => {
                self.step(-1);
                Action::None
            }
            KeyCode::Char('s') => {
                self.engine.stop();
                Action::None
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                self.set_volume(self.volume.saturating_add(5));
                Action::None
            }
            KeyCode::Char('-') | KeyCode::Char('_') => {
                self.set_volume(self.volume.saturating_sub(5));
                Action::None
            }
            _ => Action::None,
        }
    }

    /// Moves the cursor through the songs.
    fn move_by(&mut self, step: isize) {
        if self.results.is_empty() {
            return;
        }
        let last = self.results.len() as isize - 1;
        let next = (self.selected as isize + step).clamp(0, last);
        self.selected = next as usize;
    }

    /// Plays the song under the cursor.
    fn play_selected(&mut self) {
        if let Some(track) = self.results.get(self.selected).cloned() {
            self.engine.play(track);
        }
    }

    /// Moves to the next or the previous song of the list.
    fn step(&mut self, step: isize) {
        if self.results.is_empty() {
            return;
        }
        let last = self.results.len() as isize - 1;
        let next = (self.selected as isize + step).clamp(0, last);
        self.selected = next as usize;
        self.play_selected();
    }

    /// Sets the loudness, in percent.
    fn set_volume(&mut self, percent: u8) {
        self.volume = percent.min(100);
        self.engine.volume(self.volume as f32 / 100.0);
    }

    /// Moves the sound forward or back, by seconds.
    fn seek(&mut self, seconds: f64) {
        self.engine.seek(seconds);
    }

    /// Takes the answer of the search thread.
    fn receive(&mut self, message: Message) {
        self.busy = false;
        match message {
            Message::Results(tracks) => {
                if tracks.is_empty() {
                    self.status = "nothing found".to_string();
                } else {
                    self.status = format!("{} songs", tracks.len());
                }
                self.results = tracks;
                self.selected = 0;
                self.version = self.version.wrapping_add(1);
            }
            Message::Failed(error) => {
                self.status = format!("the search failed: {error}");
            }
        }
    }

    /// Moves on when the song that was playing has ended.
    fn check_finished(&mut self) {
        let playing = self.engine.now_playing();
        // A stream that stops before the song is over is a failure, not the end of
        // the song: the engine is given the chance to fetch it again, and the
        // playlist must not move on as if the music had finished.
        if playing.status == Status::Playing
            && playing.finished.load(std::sync::atomic::Ordering::Relaxed)
            && !playing.ended_early()
        {
            if self.selected + 1 < self.results.len() {
                self.step(1);
            } else {
                self.engine.stop();
            }
        }
    }
}

/// Starts the thread that does the searching.
fn searcher() -> (Sender<String>, Receiver<Message>) {
    let (queries, inbox) = mpsc::channel::<String>();
    let (answers, outbox) = mpsc::channel::<Message>();

    std::thread::Builder::new()
        .name("search".to_string())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(_) => return,
            };
            while let Ok(query) = inbox.recv() {
                let answer = runtime.block_on(async move {
                    match ytm::search(&query).await {
                        Ok(tracks) => Message::Results(tracks),
                        Err(error) => Message::Failed(format!("{error:#}")),
                    }
                });
                if answers.send(answer).is_err() {
                    break;
                }
            }
        })
        .expect("the search thread");

    (queries, outbox)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(title: &str) -> Track {
        Track {
            id: title.to_string(),
            title: title.to_string(),
            artist: "someone".to_string(),
            album: None,
            duration: Some(100),
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: ratatui::crossterm::event::KeyEventState::NONE,
        }
    }

    /// The interface without a sound device, so the keys can be tried out.
    fn app() -> App {
        let engine = Engine::new().expect("the sound device");
        App {
            input: String::new(),
            editing: false,
            busy: false,
            results: vec![track("one"), track("two"), track("three")],
            selected: 0,
            status: String::new(),
            engine,
            volume: 80,
            tick: 0,
            version: 0,
        }
    }

    #[test]
    fn the_search_box_is_opened_by_a_slash() {
        let mut app = app();
        assert!(matches!(app.key(key(KeyCode::Char('/'))), Action::None));
        assert!(app.editing);
    }

    #[test]
    fn what_is_typed_ends_up_in_the_box() {
        let mut app = app();
        app.key(key(KeyCode::Char('/')));
        for letter in "daft".chars() {
            app.key(key(KeyCode::Char(letter)));
        }
        assert_eq!(app.input, "daft");
        app.key(key(KeyCode::Backspace));
        assert_eq!(app.input, "daf");
    }

    #[test]
    fn the_search_runs_when_the_box_is_confirmed() {
        let mut app = app();
        app.key(key(KeyCode::Char('/')));
        for letter in "punk".chars() {
            app.key(key(KeyCode::Char(letter)));
        }
        assert!(matches!(app.key(key(KeyCode::Enter)), Action::Search));
        assert!(!app.editing);
        assert!(app.busy);
    }

    #[test]
    fn the_cursor_stays_inside_the_list() {
        let mut app = app();
        app.key(key(KeyCode::Up));
        assert_eq!(app.selected, 0);
        for _ in 0..5 {
            app.key(key(KeyCode::Down));
        }
        assert_eq!(app.selected, 2);
    }

    #[test]
    fn the_loudness_stays_inside_its_limits() {
        let mut app = app();
        for _ in 0..40 {
            app.key(key(KeyCode::Char('+')));
        }
        assert_eq!(app.volume, 100);
        for _ in 0..80 {
            app.key(key(KeyCode::Char('-')));
        }
        assert_eq!(app.volume, 0);
    }

    #[test]
    fn the_loudness_moves_in_even_steps() {
        let mut app = app();
        assert_eq!(app.volume, 80);
        let mut seen = vec![app.volume];
        for _ in 0..20 {
            app.key(key(KeyCode::Char('-')));
            seen.push(app.volume);
        }
        // Every step is five percent, until the silence, and then nothing.
        let steps: Vec<i32> = seen
            .windows(2)
            .map(|pair| pair[0] as i32 - pair[1] as i32)
            .collect();
        assert!(
            steps.iter().all(|step| *step == 5 || *step == 0),
            "{steps:?}"
        );
        assert_eq!(steps.iter().filter(|step| **step == 5).count(), 16);
        assert_eq!(*seen.last().unwrap(), 0);
    }

    #[test]
    fn the_loudness_moves_up_in_even_steps() {
        let mut app = app();
        let mut seen = vec![app.volume];
        for _ in 0..10 {
            app.key(key(KeyCode::Char('+')));
            seen.push(app.volume);
        }
        let steps: Vec<i32> = seen
            .windows(2)
            .map(|pair| pair[1] as i32 - pair[0] as i32)
            .collect();
        assert!(
            steps.iter().all(|step| *step == 5 || *step == 0),
            "{steps:?}"
        );
        assert_eq!(steps.iter().filter(|step| **step == 5).count(), 4);
        assert_eq!(*seen.last().unwrap(), 100);
    }

    #[test]
    fn the_letters_of_the_box_do_not_steer_the_player() {
        let mut app = app();
        app.key(key(KeyCode::Char('/')));
        app.key(key(KeyCode::Char('q')));
        assert_eq!(app.input, "q");
        assert!(!matches!(app.key(key(KeyCode::Char('q'))), Action::Quit));
    }

    #[test]
    fn the_picture_tells_when_the_interface_has_to_be_drawn() {
        let mut app = app();
        let first = app.snapshot();
        assert_eq!(first, app.snapshot(), "nothing changed");

        app.key(key(KeyCode::Char('+')));
        assert_ne!(first, app.snapshot(), "the loudness changed");

        app.key(key(KeyCode::Char('/')));
        assert_ne!(first, app.snapshot(), "the search box opened");
    }

    #[test]
    fn a_new_list_of_songs_is_never_the_old_one() {
        let mut app = app();
        let first = app.snapshot();
        app.receive(Message::Results(vec![track("four"), track("five")]));
        assert_ne!(first, app.snapshot(), "the songs are the same list");
    }

    #[test]
    #[ignore = "it needs the network and a sound device"]
    fn the_sound_can_be_moved() {
        let engine = Engine::new().expect("the sound device");
        let track = Track {
            id: "5NV6Rdv1a3I".to_string(),
            title: "One More Time (Radio Edit)".to_string(),
            artist: "Daft Punk".to_string(),
            album: None,
            duration: Some(321),
        };
        engine.play(track);
        std::thread::sleep(Duration::from_secs(6));
        let before = engine.now_playing();
        println!(
            "before: {:?} elapsed {:.1}",
            before.status,
            before.elapsed()
        );

        engine.seek(2.0);
        std::thread::sleep(Duration::from_millis(300));
        let during = engine.now_playing();
        println!(
            "during: {:?} elapsed {:.1}",
            during.status,
            during.elapsed()
        );

        std::thread::sleep(Duration::from_secs(6));
        let after = engine.now_playing();
        println!("after:  {:?} elapsed {:.1}", after.status, after.elapsed());
        assert!(
            after.elapsed() > before.elapsed() + 2.0,
            "the sound did not move: {} -> {}",
            before.elapsed(),
            after.elapsed()
        );
    }
}
