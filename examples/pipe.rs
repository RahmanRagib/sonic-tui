//! A check of the stream pipeline on its own: yt-dlp into ffmpeg, and the bytes that come out.

use std::io::{Read, Write};
use std::process::{Command, Stdio};

fn main() {
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "https://music.youtube.com/watch?v=wU26xVT_vBU".to_string());

    let mut downloader = Command::new("yt-dlp")
        .args([
            "--no-playlist",
            "--no-warnings",
            "--quiet",
            "--no-progress",
            "-f",
            "bestaudio/best",
            "-o",
            "-",
        ])
        .arg(&url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("yt-dlp");

    let mut decoder = Command::new("ffmpeg")
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
            "44100",
            "pipe:1",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("ffmpeg");

    let mut source = downloader.stdout.take().unwrap();
    let mut input = decoder.stdin.take().unwrap();
    let mut output = decoder.stdout.take().unwrap();

    let copy = std::thread::spawn(move || {
        let mut buffer = vec![0u8; 64 * 1024];
        let mut total = 0u64;
        loop {
            match source.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(read) => {
                    total += read as u64;
                    if input.write_all(&buffer[..read]).is_err() {
                        break;
                    }
                    println!("sent {total} bytes");
                }
            }
        }
        let _ = input.flush();
        println!("the stream ended after {total} bytes");
    });

    let mut buffer = vec![0u8; 32 * 1024];
    let mut total = 0u64;
    loop {
        match output.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                total += read as u64;
                println!("decoded {total} bytes");
            }
        }
    }
    let _ = copy.join();
    let _ = downloader.wait();
    let _ = decoder.wait();
}
