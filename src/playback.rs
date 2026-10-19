//! The audio engine.
//!
//! Sonora's YouTube provider hands the stream to its own engine and lets the engine feed the
//! decoder. Here the same shape is kept, but the stream comes from `yt-dlp` (which resolves
//! YouTube's addresses and signatures) and is decoded by `ffmpeg`, which hands raw PCM to
//! `rodio`.
//!
//! ```text
//! music.youtube.com  --yt-dlp-->  audio stream  --ffmpeg-->  s16le PCM  --rodio--> speakers
//! ```
//!
//! No account is used anywhere along that line: every track is fetched as a guest.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command as Process, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use rodio::source::Source;
use rodio::{DeviceSinkBuilder, Player};

use crate::ytm::Track;

/// The rate the decoder hands the samples over at.
const SAMPLE_RATE: u32 = 44_100;
/// The number of channels the decoder hands the samples over in.
const CHANNELS: u16 = 2;
/// How often the engine looks at the sound it is playing.
const TICK: std::time::Duration = std::time::Duration::from_millis(250);
/// How many times a stream is fetched again when YouTube cuts it short.
const ATTEMPTS: u32 = 3;
/// The clients of YouTube the stream is fetched with.
///
/// The web client, which is the one yt-dlp reaches for first, is asked to sign in as
/// soon as YouTube notices a guest that fetches too much. The clients of the apps ask
/// for nothing, so they are the ones used here.
const CLIENTS: &str = "youtube:player_client=android,mweb";

/// What the engine is doing with the sound.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    Idle,
    Loading,
    Playing,
    Paused,
    Failed,
}

/// What is on the turntable right now.
#[derive(Clone, Debug, Default)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub status: Status,
    /// The running time of the track, when the catalogue stated one.
    pub duration: Option<u32>,
    /// The samples played so far, counted by the source.
    pub played: Arc<AtomicU64>,
    /// Set once the stream has ended, so the player can move on.
    pub finished: Arc<AtomicBool>,
    /// The reason the last attempt failed, if it did.
    pub error: Option<String>,
    /// What the tools behind the stream had to say.
    pub notes: Arc<Mutex<String>>,
    /// The bytes fetched from YouTube.
    pub copied: Arc<AtomicU64>,
    /// The bytes the decoder handed over.
    pub decoded: Arc<AtomicU64>,
}

impl NowPlaying {
    /// The seconds played so far.
    pub fn elapsed(&self) -> f64 {
        seconds(self.played.load(Ordering::Relaxed))
    }

    /// Whether the sound stopped before the song was over.
    ///
    /// A stream that dies at once is not the end of the song: it is a failure, and
    /// the player must not walk to the next one as if the music had finished.
    pub fn ended_early(&self) -> bool {
        ended_early(self.elapsed(), self.duration)
    }
}

/// Whether the sound stopped before the song was over.
fn ended_early(played: f64, duration: Option<u32>) -> bool {
    match duration {
        Some(duration) => played + 2.0 < duration as f64,
        None => played < 0.5,
    }
}

/// The seconds a number of samples stands for. The samples are interleaved, so a frame of
/// stereo sound is two of them.
fn seconds(samples: u64) -> f64 {
    samples as f64 / (SAMPLE_RATE as f64 * CHANNELS as f64)
}

/// What the player asks of the engine.
pub enum Command {
    Play(Track),
    Pause,
    Resume,
    Stop,
    Volume(f32),
    Quit,
}

/// The engine, held by the interface. Every call is a message to the audio thread.
pub struct Engine {
    commands: Sender<Command>,
    state: Arc<Mutex<NowPlaying>>,
}

impl Engine {
    /// Starts the audio thread and opens the sound device.
    pub fn new() -> anyhow::Result<Arc<Engine>> {
        let (commands, inbox) = mpsc::channel();
        let state = Arc::new(Mutex::new(NowPlaying::default()));

        let thread_state = state.clone();
        thread::Builder::new()
            .name("audio".to_string())
            .spawn(move || audio_thread(inbox, thread_state))?;

        Ok(Arc::new(Engine { commands, state }))
    }

    /// Starts a track, replacing whatever was playing.
    pub fn play(&self, track: Track) {
        let _ = self.commands.send(Command::Play(track));
    }

    pub fn pause(&self) {
        let _ = self.commands.send(Command::Pause);
    }

    pub fn resume(&self) {
        let _ = self.commands.send(Command::Resume);
    }

    pub fn stop(&self) {
        let _ = self.commands.send(Command::Stop);
    }

    /// Sets the loudness, from 0 to 1.
    pub fn volume(&self, volume: f32) {
        let _ = self.commands.send(Command::Volume(volume));
    }

    /// Ends the sound and the thread behind it.
    pub fn quit(&self) {
        let _ = self.commands.send(Command::Quit);
    }

    /// What is playing right now.
    pub fn now_playing(&self) -> NowPlaying {
        self.state.lock().expect("the engine state").clone()
    }
}

/// The audio thread: it owns the sound device, the player and the running processes.
fn audio_thread(inbox: Receiver<Command>, state: Arc<Mutex<NowPlaying>>) {
    let mut device = match DeviceSinkBuilder::open_default_sink() {
        Ok(device) => device,
        Err(error) => {
            let mut state = state.lock().expect("the engine state");
            state.status = Status::Failed;
            state.error = Some(format!("no sound device: {error}"));
            return;
        }
    };
    // The player says goodbye by itself; there is no need to say it twice.
    device.log_on_drop(false);

    let mixer = device.mixer();
    let mut player: Option<Player> = None;
    let mut session: Option<Session> = None;

    loop {
        let command = match inbox.recv_timeout(TICK) {
            Ok(command) => command,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // YouTube refuses a stream now and then with a 403. When the sound stops
                // before the song is over, the stream is fetched again.
                let Some(current) = &session else { continue };
                if !current.finished.load(Ordering::Relaxed) || !current.cut_short() {
                    continue;
                }
                let track = current.track.clone();
                let retries = current.retries + 1;
                let copied = current.copied.load(Ordering::Relaxed);
                let notes = current.notes.lock().expect("the notes").clone();
                if let Some(previous) = session.take() {
                    previous.kill();
                }
                player = None;

                // Nothing came through, or there is nothing left to try: the player
                // says what went wrong instead of walking to the next song.
                if copied == 0 || retries > ATTEMPTS {
                    let mut state = state.lock().expect("the engine state");
                    state.status = Status::Failed;
                    state.error = Some(failure(&notes));
                    continue;
                }

                {
                    let mut state = state.lock().expect("the engine state");
                    state.status = Status::Loading;
                    state.error = None;
                }
                launch(&track, mixer, &state, retries, &mut session, &mut player);
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };

        match command {
            Command::Play(track) => {
                if let Some(previous) = session.take() {
                    previous.kill();
                }
                player = None;

                {
                    let mut state = state.lock().expect("the engine state");
                    state.title = track.title.clone();
                    state.artist = track.artist.clone();
                    state.album = track.album.clone().unwrap_or_default();
                    state.duration = track.duration;
                    state.status = Status::Loading;
                    state.error = None;
                }

                launch(&track, mixer, &state, 0, &mut session, &mut player);
            }
            Command::Pause => {
                if let Some(player) = &player {
                    player.pause();
                }
                let mut state = state.lock().expect("the engine state");
                if state.status == Status::Playing {
                    state.status = Status::Paused;
                }
            }
            Command::Resume => {
                if let Some(player) = &player {
                    player.play();
                }
                let mut state = state.lock().expect("the engine state");
                if state.status == Status::Paused {
                    state.status = Status::Playing;
                }
            }
            Command::Stop => {
                if let Some(previous) = session.take() {
                    previous.kill();
                }
                if let Some(player) = &player {
                    player.stop();
                }
                player = None;
                let mut state = state.lock().expect("the engine state");
                state.status = Status::Idle;
                state.title.clear();
                state.artist.clear();
                state.played = Arc::new(AtomicU64::new(0));
            }
            Command::Volume(volume) => {
                if let Some(player) = &player {
                    player.set_volume(volume.clamp(0.0, 1.0));
                }
            }
            Command::Quit => {
                if let Some(previous) = session.take() {
                    previous.kill();
                }
                break;
            }
        }
    }
}

/// Starts a track and hands the sound to the player.
fn launch(
    track: &Track,
    mixer: &rodio::mixer::Mixer,
    state: &Arc<Mutex<NowPlaying>>,
    retries: u32,
    session: &mut Option<Session>,
    player: &mut Option<Player>,
) {
    let new_player = Player::connect_new(mixer);
    match Session::start(track, &new_player, retries) {
        Ok(started) => {
            {
                let mut state = state.lock().expect("the engine state");
                state.played = started.played.clone();
                state.finished = started.finished.clone();
                state.notes = started.notes.clone();
                state.copied = started.copied.clone();
                state.decoded = started.decoded.clone();
            }
            *session = Some(started);
            new_player.set_volume(1.0);
            new_player.play();
            *player = Some(new_player);
            let mut state = state.lock().expect("the engine state");
            state.status = Status::Playing;
        }
        Err(error) => {
            let mut state = state.lock().expect("the engine state");
            state.status = Status::Failed;
            state.error = Some(format!("{error:#}"));
        }
    }
}

/// The reason the stream did not come, in the words of the tools behind it.
fn failure(notes: &str) -> String {
    let mut lines = notes.lines().map(str::trim).filter(|line| !line.is_empty());
    match lines.next() {
        Some(line) => line.to_string(),
        None => "the stream did not come".to_string(),
    }
}

/// A running stream: the two processes and the counters of the track being played.
struct Session {
    /// The track the stream belongs to, kept for another attempt.
    track: Track,
    /// The address fetcher, killed when the track changes.
    downloader: Child,
    /// The decoder, killed when the track changes.
    decoder: Child,
    played: Arc<AtomicU64>,
    finished: Arc<AtomicBool>,
    notes: Arc<Mutex<String>>,
    copied: Arc<AtomicU64>,
    decoded: Arc<AtomicU64>,
    /// How many times the stream was fetched again.
    retries: u32,
}

impl Session {
    /// Whether the sound stopped before the song was over.
    fn cut_short(&self) -> bool {
        ended_early(
            seconds(self.played.load(Ordering::Relaxed)),
            self.track.duration,
        )
    }

    /// Starts the pipeline for a track and hands the samples to the player.
    fn start(track: &Track, player: &Player, retries: u32) -> anyhow::Result<Session> {
        let notes = Arc::new(Mutex::new(String::new()));

        let mut downloader = Process::new("yt-dlp")
            .args([
                "--no-playlist",
                "--no-warnings",
                "--quiet",
                "--no-progress",
                "--extractor-args",
                CLIENTS,
                "-f",
                "bestaudio/best",
                "-o",
                "-",
            ])
            .arg(track.watch_url())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| anyhow::anyhow!("yt-dlp was not found on the path"))?;

        let mut decoder = Process::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-i",
                "pipe:0",
                "-f",
                "s16le",
                "-ac",
                "2",
                "-ar",
                &SAMPLE_RATE.to_string(),
                "pipe:1",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| anyhow::anyhow!("ffmpeg was not found on the path"))?;

        let download = downloader
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("the stream had no output"))?;
        let input = decoder
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("the decoder had no input"))?;
        let output = decoder
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("the decoder had no output"))?;

        // What the tools say about the stream, kept for the interface.
        for stream in [downloader.stderr.take(), decoder.stderr.take()]
            .into_iter()
            .flatten()
        {
            let notes = notes.clone();
            thread::Builder::new()
                .name("notes".to_string())
                .spawn(move || {
                    for line in BufReader::new(stream).lines().map_while(Result::ok) {
                        let mut notes = notes.lock().expect("the notes");
                        if !notes.is_empty() {
                            notes.push('\n');
                        }
                        notes.push_str(&line);
                    }
                })?;
        }

        // The address fetcher writes into the decoder, which is the whole stream.
        let copied = Arc::new(AtomicU64::new(0));
        let copied_thread = copied.clone();
        thread::Builder::new()
            .name("stream".to_string())
            .spawn(move || {
                let mut download = download;
                let mut input = input;
                let mut buffer = vec![0u8; 64 * 1024];
                loop {
                    match download.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(read) => {
                            copied_thread.fetch_add(read as u64, Ordering::Relaxed);
                            if input.write_all(&buffer[..read]).is_err() {
                                break;
                            }
                        }
                    }
                }
                let _ = input.flush();
            })?;

        // The decoder's samples go to the sound device.
        let (outbox, inbox) = mpsc::channel::<Vec<f32>>();
        let played = Arc::new(AtomicU64::new(0));
        let finished = Arc::new(AtomicBool::new(false));
        let decoded = Arc::new(AtomicU64::new(0));

        let reader_finished = finished.clone();
        let reader_decoded = decoded.clone();
        thread::Builder::new()
            .name("decoder".to_string())
            .spawn(move || {
                let mut output = output;
                let mut buffer = vec![0u8; 32 * 1024];
                loop {
                    match output.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(read) => {
                            reader_decoded.fetch_add(read as u64, Ordering::Relaxed);
                            let (pairs, _) = buffer[..read].as_chunks::<2>();
                            let chunk: Vec<f32> = pairs
                                .iter()
                                .map(|pair| {
                                    let value = i16::from_le_bytes([pair[0], pair[1]]);
                                    value as f32 / f32::from(i16::MAX)
                                })
                                .collect();
                            if outbox.send(chunk).is_err() {
                                break;
                            }
                        }
                    }
                }
                // The song is over only once the last sample has been played, which is
                // when the source runs out, not when the download does.
                drop(reader_finished);
            })?;

        let source = PcmSource {
            inbox,
            buffer: Vec::new(),
            position: 0,
            played: played.clone(),
            finished: finished.clone(),
        };
        player.append(source);

        Ok(Session {
            track: track.clone(),
            downloader,
            decoder,
            played,
            finished,
            notes,
            copied,
            decoded,
            retries,
        })
    }

    /// Ends the stream and the processes behind it.
    fn kill(mut self) {
        let _ = self.downloader.kill();
        let _ = self.decoder.kill();
        let _ = self.downloader.wait();
        let _ = self.decoder.wait();
        self.finished.store(true, Ordering::Relaxed);
    }
}

/// The samples the decoder produced, as a source for the sound device.
struct PcmSource {
    inbox: Receiver<Vec<f32>>,
    buffer: Vec<f32>,
    position: usize,
    played: Arc<AtomicU64>,
    finished: Arc<AtomicBool>,
}

impl Iterator for PcmSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        loop {
            if self.position < self.buffer.len() {
                let sample = self.buffer[self.position];
                self.position += 1;
                self.played.fetch_add(1, Ordering::Relaxed);
                return Some(sample);
            }
            match self.inbox.recv() {
                Ok(chunk) => {
                    self.buffer = chunk;
                    self.position = 0;
                }
                Err(_) => {
                    self.finished.store(true, Ordering::Relaxed);
                    return None;
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.buffer.len() - self.position;
        (remaining, None)
    }
}

impl Source for PcmSource {
    fn current_span_len(&self) -> Option<usize> {
        // The stream has no known length: the decoder fills it as it goes.
        None
    }

    fn channels(&self) -> rodio::ChannelCount {
        rodio::ChannelCount::new(CHANNELS).expect("two channels")
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        rodio::SampleRate::new(SAMPLE_RATE).expect("a sample rate")
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_song_that_stops_early_is_not_the_end_of_the_song() {
        assert!(ended_early(0.0, Some(321)), "nothing was played");
        assert!(ended_early(30.0, Some(321)), "the song was cut short");
        assert!(!ended_early(321.0, Some(321)), "the song was over");
        assert!(
            ended_early(0.4, None),
            "a song with no length played nothing"
        );
        assert!(!ended_early(2.0, None), "a song with no length was played");
    }

    #[test]
    fn the_reason_the_stream_did_not_come_is_kept() {
        let notes = "ERROR: [youtube] 5NV6Rdv1a3I: Sign in to confirm you are not a bot.";
        assert_eq!(failure(notes), notes);
        assert_eq!(failure(""), "the stream did not come");
        assert_eq!(failure("\n  \n"), "the stream did not come");
    }
}
