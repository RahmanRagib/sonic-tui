//! The YouTube Music catalogue, reached the way Sonora's YouTube provider reaches it:
//! the InnerTube `search` endpoint with a guest client. No account, no sign-in, no cookie.
//!
//! The response is a tree of renderers; the songs live in `musicShelfRenderer` entries, each
//! item being a `musicResponsiveListItemRenderer` whose flex columns carry the title, the
//! artist, the album and the running time, and whose `playlistItemData` carries the video id
//! the stream is fetched from.

use anyhow::{Context as _, Result};
use serde_json::{Value, json};

/// The public InnerTube key the music web client uses. It is not a secret and needs no
/// account: it only identifies the client the request claims to be.
const API_KEY: &str = "AIzaSyC9XL3ZjWddXya6X74dJoCTL-WEYFDNX30";
const CLIENT: &str = "WEB_REMIX";
const CLIENT_VERSION: &str = "1.20250515.01.00";
/// The `params` that narrow a search to songs, the way the "Songs" tab does.
const SONGS: &str = "EgWKAQIIAQ%3D%3D";
const ENDPOINT: &str = "https://music.youtube.com/youtubei/v1/search";

/// One song, as the catalogue describes it.
#[derive(Clone, Debug)]
pub struct Track {
    /// The video id the audio stream is fetched from.
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    /// The running time in seconds, when the catalogue states one.
    pub duration: Option<u32>,
}

impl Track {
    /// The address the stream is fetched from, on YouTube Music.
    pub fn watch_url(&self) -> String {
        format!("https://music.youtube.com/watch?v={}", self.id)
    }
}

/// Searches YouTube Music for songs matching `query`.
pub async fn search(query: &str) -> Result<Vec<Track>> {
    let body = json!({
        "context": {
            "client": {
                "clientName": CLIENT,
                "clientVersion": CLIENT_VERSION,
                "hl": "en",
                "gl": "US",
            }
        },
        "query": query,
        "params": SONGS,
    });

    let http = reqwest::Client::builder()
        .user_agent(concat!(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 ",
            "(KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36"
        ))
        .build()
        .context("the http client")?;

    let response = http
        .post(format!("{ENDPOINT}?key={API_KEY}"))
        .header("X-YouTube-Client-Name", "67")
        .header("X-YouTube-Client-Version", CLIENT_VERSION)
        .header("Origin", "https://music.youtube.com")
        .header("Accept-Language", "en-US,en;q=0.9")
        .json(&body)
        .send()
        .await
        .context("the search request to YouTube Music")?
        .error_for_status()
        .context("YouTube Music refused the search")?
        .json::<Value>()
        .await
        .context("the search answer")?;

    Ok(parse_tracks(&response))
}

/// Pulls the songs out of the answer.
fn parse_tracks(response: &Value) -> Vec<Track> {
    let mut tracks = Vec::new();
    for shelf in find_all(response, "musicShelfRenderer") {
        let Some(items) = shelf.get("contents").and_then(Value::as_array) else {
            continue;
        };
        for item in items {
            let Some(renderer) = item.get("musicResponsiveListItemRenderer") else {
                continue;
            };
            if let Some(track) = parse_track(renderer) {
                tracks.push(track);
            }
        }
    }
    tracks
}

/// Reads one song out of its renderer.
fn parse_track(renderer: &Value) -> Option<Track> {
    let id = renderer
        .get("playlistItemData")
        .and_then(|data| data.get("videoId"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            // Some items only carry the id inside the overlay's watch endpoint.
            renderer
                .get("overlay")
                .and_then(|overlay| overlay.get("musicItemThumbnailOverlayRenderer"))
                .and_then(|overlay| overlay.get("content"))
                .and_then(|content| content.get("musicPlayButtonRenderer"))
                .and_then(|button| button.get("playNavigationEndpoint"))
                .and_then(|endpoint| endpoint.get("watchEndpoint"))
                .and_then(|endpoint| endpoint.get("videoId"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })?;

    let columns = flex_columns(renderer);

    // The first column holds the title, the second the artist, the album and the time.
    let title = columns
        .first()
        .map(|runs| join_runs(runs))
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| id.clone());

    let detail = columns.get(1).map(|runs| runs.to_vec()).unwrap_or_default();

    let artist = detail
        .iter()
        .find(|run| {
            run.get("navigationEndpoint")
                .and_then(|endpoint| endpoint.get("browseEndpoint"))
                .and_then(|endpoint| endpoint.get("browseEndpointContextSupportedConfigs"))
                .and_then(|configs| configs.get("browseEndpointContextMusicConfig"))
                .and_then(|config| config.get("pageType"))
                .and_then(Value::as_str)
                == Some("MUSIC_PAGE_TYPE_ARTIST")
        })
        .and_then(|run| run.get("text").and_then(Value::as_str))
        .unwrap_or_default()
        .to_string();

    let album = detail
        .iter()
        .find(|run| {
            run.get("navigationEndpoint")
                .and_then(|endpoint| endpoint.get("browseEndpoint"))
                .and_then(|endpoint| endpoint.get("browseEndpointContextSupportedConfigs"))
                .and_then(|configs| configs.get("browseEndpointContextMusicConfig"))
                .and_then(|config| config.get("pageType"))
                .and_then(Value::as_str)
                == Some("MUSIC_PAGE_TYPE_ALBUM")
        })
        .and_then(|run| run.get("text").and_then(Value::as_str))
        .map(str::to_string);

    let duration = detail
        .iter()
        .filter_map(|run| run.get("text").and_then(Value::as_str))
        .find_map(parse_time);

    Some(Track {
        id,
        title,
        artist,
        album,
        duration,
    })
}

/// The text runs of every flex column of a list item.
fn flex_columns(renderer: &Value) -> Vec<Vec<&Value>> {
    let Some(columns) = renderer.get("flexColumns").and_then(Value::as_array) else {
        return Vec::new();
    };
    columns
        .iter()
        .map(|column| {
            column
                .get("musicResponsiveListItemFlexColumnRenderer")
                .and_then(|column| column.get("text"))
                .and_then(|text| text.get("runs"))
                .and_then(Value::as_array)
                .map(|runs| runs.iter().collect())
                .unwrap_or_default()
        })
        .collect()
}

/// The text of a run list, with the separators dropped.
fn join_runs(runs: &[&Value]) -> String {
    runs.iter()
        .filter_map(|run| run.get("text").and_then(Value::as_str))
        .filter(|text| !text.trim().is_empty() && *text != " • ")
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

/// Reads a running time, written either as `3:20` or as `1:02:03`.
fn parse_time(text: &str) -> Option<u32> {
    let text = text.trim();
    if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit() || c == ':') {
        return None;
    }
    let mut seconds = 0u32;
    for part in text.split(':') {
        seconds = seconds
            .checked_mul(60)?
            .checked_add(part.parse::<u32>().ok()?)?;
    }
    Some(seconds)
}

/// Every value in the tree that sits under `key`, however deep it is.
fn find_all<'a>(value: &'a Value, key: &str) -> Vec<&'a Value> {
    let mut found = Vec::new();
    walk(value, key, &mut found);
    found
}

fn walk<'a>(value: &'a Value, key: &str, found: &mut Vec<&'a Value>) {
    match value {
        Value::Object(map) => {
            for (name, child) in map {
                if name == key {
                    found.push(child);
                }
                walk(child, key, found);
            }
        }
        Value::Array(items) => {
            for item in items {
                walk(item, key, found);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_running_time() {
        assert_eq!(parse_time("3:20"), Some(200));
        assert_eq!(parse_time("1:02:03"), Some(3723));
        assert_eq!(parse_time("853M plays"), None);
    }

    #[test]
    fn reads_a_song_out_of_its_renderer() {
        let renderer = json!({
            "playlistItemData": { "videoId": "abc123" },
            "flexColumns": [
                { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [
                    { "text": "One More Time" }
                ] } } },
                { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [
                    { "text": "Daft Punk", "navigationEndpoint": { "browseEndpoint": {
                        "browseEndpointContextSupportedConfigs": { "browseEndpointContextMusicConfig": {
                            "pageType": "MUSIC_PAGE_TYPE_ARTIST" } } } } },
                    { "text": " • " },
                    { "text": "Discovery", "navigationEndpoint": { "browseEndpoint": {
                        "browseEndpointContextSupportedConfigs": { "browseEndpointContextMusicConfig": {
                            "pageType": "MUSIC_PAGE_TYPE_ALBUM" } } } } },
                    { "text": " • " },
                    { "text": "5:21" }
                ] } } }
            ]
        });
        let track = parse_track(&renderer).expect("a song");
        assert_eq!(track.id, "abc123");
        assert_eq!(track.title, "One More Time");
        assert_eq!(track.artist, "Daft Punk");
        assert_eq!(track.album.as_deref(), Some("Discovery"));
        assert_eq!(track.duration, Some(321));
    }
}
