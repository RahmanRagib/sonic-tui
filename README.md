# sonic-tui

**A music streaming client for the terminal.** Search YouTube Music, pick a song, and it
plays — no account, no sign-in, no configuration.

```
┌ SonicSearch (press /) ───────────────────────────────────────────────────────────────┐
│› daft punk one more time                                                             │
└──────────────────────────────────────────────────────────────────────────────────────┘
┌ Songs · 12 songs ────────────────────────────────────────────────────────────────────┐
│  One More Time (Radio Edit)  Daft Punk  5:21                                         │
│♪ One More Time (feat. Alida)  Robin Schulz  3:20                                     │
│  One More Time  3:45                                                                 │
│                                                                                      │
│                                                                                      │
└──────────────────────────────────────────────────────────────────────────────────────┘
┌ Now playing ─────────────────────────────────────────────────────────────────────────┐
│One More Time (Radio Edit) — Daft Punk · Discovery                                      │
└──────────────────────────────────────────────────────────────────────────────────────┘
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
0:42 / 5:21    vol  80%                                                                 
 Keys ──────────────────────────────────────────────────────────────────────────────────
↵ play  space pause  n next  p prev  ←/→ seek  -/+ vol  s stop  / search  q quit        
```

## What it is

* **A catalogue** — YouTube Music, reached as a *guest*. Nothing to sign in to, no cookie,
  no key to paste.
* **An engine** — `yt-dlp` resolves and signs the stream, `ffmpeg` decodes it, `rodio`
  plays it.
* **An interface** — a small terminal UI: a search box, the songs it found, and what is
  playing right now.
* **A mood** — a matrix of moss green, the spring green of the keys under a mandarin title,
  and a bar that fills from the left at the speed of the music.

## Installing it

### The easy way

```sh
cargo install sonic-tui
sonic-tui
```

The binary lands in `~/.cargo/bin`, which is already on the path on most machines. On
Windows the same command works and the program is `sonic-tui.exe`.

### From GitHub, without crates.io

```sh
cargo install --git https://github.com/RahmanRagib/sonic-tui --locked
```

### A prebuilt binary

Every release carries a ready-made archive for Windows and Linux. Take the one for
your system, put `sonic-tui` (or `sonic-tui.exe`) anywhere on the path, and it runs.

### From a clone

```sh
git clone https://github.com/RahmanRagib/sonic-tui
cd sonic-tui
cargo install --path .
```

### The Windows shortcut

`sonic-tui.cmd` sits in the root of the repository. It builds the player first, so the
program is never out of date, and then runs it:

```sh
cd sonic-tui
.\sonic-tui            # the player, built and run
```

To start it from anywhere, put a small `sonic.cmd` in a folder on the path that calls it:

```cmd
@echo off
call "C:\path\to\sonic-tui\sonic-tui.cmd" %*
```

Then `sonic` runs the player from any directory, with the current code in it.

## What it needs

Two helpers have to be on the path, because they do what a browser would do:

| Helper | What it does |
| --- | --- |
| [yt-dlp](https://github.com/yt-dlp/yt-dlp) | resolves the address of the stream and signs it |
| [ffmpeg](https://ffmpeg.org) | decodes the stream into sound |

The stream is fetched with the client of the YouTube apps, not the one of the website,
which asks a guest to sign in as soon as it notices one. No account is used either way.

The search is made with the public key of the YouTube Music web client. That key belongs
to nobody in particular — it is in the page of YouTube Music for every visitor — but a key
of one's own can be used instead, which is the way out if the shared one is ever refused:

```sh
set YOUTUBE_API_KEY=your-key-here    # Windows
YOUTUBE_API_KEY=your-key-here sonic   # Linux
```

```sh
winget install yt-dlp.yt-dlp          # Windows
sudo apt install yt-dlp ffmpeg         # Debian / Ubuntu
```

Without them the interface still runs and the search still works; only the sound needs
them. When a stream will not come, the player says so instead of moving on to the next
song.

## Using it

```sh
sonic-tui                       # the player, with an empty search box
sonic-tui daft punk             # the player, searching for "daft punk" right away
```

| Key | What it does |
| --- | --- |
| `/` | open the search box |
| `Enter` | in the box: search · in the list: play the song under the cursor |
| `Esc` | leave the search box |
| `↑` `↓` (or `j` `k`) | move the cursor through the songs |
| `←` `→` | move the sound forward or back, two seconds at a time |
| `Space` | pause and resume |
| `n` / `p` | the next or the previous song of the list |
| `-` / `+` | quieter and louder, five percent at a time |
| `s` | stop |
| `q` | quit |

The songs the search found are the playlist: `n` and `p` walk through them, and the player
moves on by itself when a song ends. The sound is moved with `←` and `→`: the stream is
fetched again, further into the song or further back, which takes a moment.

## How it works

```
music.youtube.com ──InnerTube search──▶ the songs
music.youtube.com ──yt-dlp──▶ the stream ──ffmpeg──▶ s16le PCM ──rodio──▶ the speakers
```

| File | What is in it |
| --- | --- |
| `src/ytm.rs` | the provider. It asks YouTube Music's own search endpoint for songs and reads the answer the way Sonora's YouTube provider does: the `musicShelfRenderer` of the answer holds `musicResponsiveListItemRenderer` items, whose flex columns carry the title, the artist, the album and the running time, and whose `playlistItemData` carries the video the stream is fetched from. A guest client, so nothing has to be signed in. |
| `src/playback.rs` | the engine. It runs the two helpers as a pipeline, hands the samples to `rodio` as a source, and keeps the counters the interface draws: how far the song has come, how loud it is, whether it is playing. When YouTube cuts a stream short with a 403, the stream is fetched again, up to three times. |
| `src/ui.rs` | the interface: a search box, the songs it found, what is playing, and the bar that fills from the left at the speed of the song. |
| `src/theme.rs` | the colours: a matrix of moss green, the spring green of the keys, the mandarin of their title. |
| `src/main.rs` | the loop that ties them together, the key handling and the command line. |

## The other modes

Three extra modes help when something is wrong:

```sh
sonic-tui --play "daft punk one more time"        # search, play, and report
sonic-tui --draw "daft punk one more time"        # draw one frame and print it
sonic-tui --keys "/daft punk one more time\r\r"   # play a script of keys, then draw
```

```sh
cargo run --example pipe    # the stream on its own
cargo run --example tone    # whether the sound device works
```

## Building and checking it

```sh
cargo build --release
cargo test
cargo clippy --all-targets
cargo fmt --all
```

The repository builds on Windows and Linux. The workflows in `.github/workflows`
run the checks on every push and attach the binaries to each tagged release.

## Licence

MIT — see [LICENSE](LICENSE).

The idea comes from [Sonora](https://github.com/sonorahq/sonora), which is under the GPL;
no code was taken from it. YouTube Music is a trademark of Google LLC, and this is an
unofficial client.
