//! A check of the sound device on its own: a tone, and the samples that come out of it.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use rodio::source::Source;
use rodio::{DeviceSinkBuilder, Player};

struct Tone {
    played: Arc<AtomicU64>,
    index: u64,
}

impl Iterator for Tone {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        self.index += 1;
        self.played.fetch_add(1, Ordering::Relaxed);
        Some((self.index as f32 * 0.01).sin() * 0.1)
    }
}

impl Source for Tone {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> rodio::ChannelCount {
        rodio::ChannelCount::new(2).unwrap()
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        rodio::SampleRate::new(44_100).unwrap()
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

fn main() {
    let device = DeviceSinkBuilder::open_default_sink().expect("the sound device");
    println!("the device is open");
    let player = Player::connect_new(device.mixer());
    let played = Arc::new(AtomicU64::new(0));
    player.append(Tone {
        played: played.clone(),
        index: 0,
    });
    player.play();
    for _ in 0..5 {
        std::thread::sleep(Duration::from_millis(500));
        println!("played {} samples", played.load(Ordering::Relaxed));
    }
}
