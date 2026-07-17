use std::path::PathBuf;

use serde_json::Value;

use crate::application::ports::{MediaProbe, ProbeError};
use crate::domain::media_info::MediaInfo;
use crate::domain::media_url::MediaUrl;
use crate::domain::quality::Quality;

/// Infrastructure adapter that probes a media URL using `yt-dlp --no-playlist -J`.
pub struct YtDlpProbe {
    binary_path: PathBuf,
}

impl YtDlpProbe {
    /// Construct a probe adapter using the given binary path.
    pub fn new(binary_path: PathBuf) -> Self {
        Self { binary_path }
    }
}

/// Build the yt-dlp argument vector for a metadata probe.
///
/// Pure function. `--no-playlist` MUST always be present: a URL carrying a
/// `&list=...` component would otherwise make yt-dlp dump the ENTIRE playlist
/// as JSON instead of the single video (PLANNING §7.1). URL is the last arg.
pub fn build_probe_args(url: &MediaUrl) -> Vec<String> {
    vec![
        "--no-playlist".to_string(),
        "-J".to_string(),
        url.as_str().to_string(),
    ]
}

impl MediaProbe for YtDlpProbe {
    fn probe(&self, url: &MediaUrl) -> Result<MediaInfo, ProbeError> {
        let output = std::process::Command::new(&self.binary_path)
            .args(build_probe_args(url))
            .output()
            .map_err(|e| ProbeError::Failed(format!("failed to spawn yt-dlp: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ProbeError::Failed(format!(
                "yt-dlp exited with {}: {}",
                output.status,
                stderr.lines().next().unwrap_or("no output")
            )));
        }

        let json: Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| ProbeError::Failed(format!("invalid JSON from yt-dlp: {e}")))?;

        parse_probe_json(&json)
    }
}

/// Parse a `serde_json::Value` (from `yt-dlp -J` stdout) into a `MediaInfo`.
///
/// Pure function — no I/O. Maps the untyped JSON to the domain struct.
/// Missing / null optional fields degrade gracefully to `None`.
/// Panics are forbidden — all errors return `Err(ProbeError::Failed(_))`.
pub fn parse_probe_json(value: &Value) -> Result<MediaInfo, ProbeError> {
    // Playlist JSON has `_type: "playlist"` — return an error (not a panic)
    if value.get("_type").and_then(Value::as_str) == Some("playlist") {
        return Err(ProbeError::Unsupported(
            "URL is a playlist; probe a single video".to_string(),
        ));
    }

    let title = value
        .get("title")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| ProbeError::Failed("missing 'title' in yt-dlp JSON".to_string()))?;

    let thumbnail_url = best_thumbnail_url(value);

    let duration_secs = value.get("duration").and_then(Value::as_f64);

    let uploader = value
        .get("uploader")
        .and_then(Value::as_str)
        .map(str::to_owned);

    // Derive available qualities from formats[].
    // Include only entries where:
    //   - vcodec is present and != "none"
    //   - height is present and non-null
    let available_qualities = {
        let mut qualities: Vec<Quality> = value
            .get("formats")
            .and_then(Value::as_array)
            .map(|formats| {
                formats
                    .iter()
                    .filter(|f| {
                        let vcodec = f.get("vcodec").and_then(Value::as_str).unwrap_or("none");
                        vcodec != "none"
                    })
                    .filter_map(|f| f.get("height").and_then(Value::as_u64))
                    .map(|h| Quality::from_height(h as u32))
                    .collect()
            })
            .unwrap_or_default();

        // Dedup (preserve unique values)
        qualities.sort_by(|a, b| {
            // Sort descending by height_cap (Best has no cap — put it last)
            let cap_a = match a {
                Quality::Best => u32::MAX,
                q => q.height_cap(),
            };
            let cap_b = match b {
                Quality::Best => u32::MAX,
                q => q.height_cap(),
            };
            cap_b.cmp(&cap_a)
        });
        qualities.dedup();
        qualities
    };

    Ok(MediaInfo {
        title,
        thumbnail_url,
        duration_secs,
        uploader,
        available_qualities,
    })
}

/// Select the highest-resolution thumbnail URL from the yt-dlp JSON.
///
/// Prefers the entry in the `thumbnails` array with the greatest reported
/// `width` (yt-dlp usually lists several resolutions); falls back to the
/// top-level `thumbnail` field when the array is absent or has no usable URL.
/// Returns `None` when no thumbnail is available.
fn best_thumbnail_url(value: &Value) -> Option<String> {
    if let Some(arr) = value.get("thumbnails").and_then(Value::as_array) {
        let widest = arr
            .iter()
            .filter_map(|t| {
                let url = t.get("url").and_then(Value::as_str)?;
                let width = t.get("width").and_then(Value::as_u64).unwrap_or(0);
                Some((width, url.to_owned()))
            })
            .max_by_key(|(width, _)| *width)
            .map(|(_, url)| url);
        if widest.is_some() {
            return widest;
        }
    }
    value
        .get("thumbnail")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── Fixture-based unit tests for parse_probe_json ──────────────────────

    // 4.1 RED → GREEN — full JSON maps all fields correctly
    #[test]
    fn test_full_json_maps_all_fields() {
        let value = json!({
            "title": "Test Video",
            "thumbnail": "https://i.ytimg.com/vi/abc/hqdefault.jpg",
            "duration": 193.0,
            "uploader": "TestChannel",
            "formats": [
                { "vcodec": "avc1.42001f", "height": 1080, "ext": "mp4" },
                { "vcodec": "vp9",         "height": 720,  "ext": "webm" },
                { "vcodec": "none",         "height": null, "ext": "m4a" }
            ]
        });

        let result = parse_probe_json(&value).expect("should succeed");
        assert_eq!(result.title, "Test Video");
        assert_eq!(
            result.thumbnail_url,
            Some("https://i.ytimg.com/vi/abc/hqdefault.jpg".to_string())
        );
        assert!((result.duration_secs.unwrap() - 193.0).abs() < 0.001);
        assert_eq!(result.uploader, Some("TestChannel".to_string()));
        assert_eq!(
            result.available_qualities,
            vec![Quality::P1080, Quality::P720]
        );
    }

    // 4.1 RED → GREEN — missing optional fields degrade to None, no panic
    #[test]
    fn test_missing_optional_fields() {
        let value = json!({
            "title": "Minimal Video",
            "formats": []
        });

        let result = parse_probe_json(&value).expect("should succeed");
        assert_eq!(result.title, "Minimal Video");
        assert!(result.thumbnail_url.is_none());
        assert!(result.duration_secs.is_none());
        assert!(result.uploader.is_none());
        assert!(result.available_qualities.is_empty());
    }

    // 4.1 RED → GREEN — audio-only formats excluded from available_qualities
    #[test]
    fn test_audio_only_excluded() {
        let value = json!({
            "title": "Audio Only Test",
            "formats": [
                { "vcodec": "none", "height": 1080, "ext": "m4a" },
                { "vcodec": "none", "height": null, "ext": "opus" },
                { "vcodec": "avc1", "height": 720,  "ext": "mp4" }
            ]
        });

        let result = parse_probe_json(&value).expect("should succeed");
        // Only the 720p video format should be present
        assert_eq!(result.available_qualities, vec![Quality::P720]);
    }

    // 4.1 RED → GREEN — duplicate heights are deduped
    #[test]
    fn test_duplicate_heights_deduped() {
        let value = json!({
            "title": "Dup Heights",
            "formats": [
                { "vcodec": "avc1", "height": 1080, "ext": "mp4" },
                { "vcodec": "vp9",  "height": 1080, "ext": "webm" },
                { "vcodec": "avc1", "height": 720,  "ext": "mp4" }
            ]
        });

        let result = parse_probe_json(&value).expect("should succeed");
        assert_eq!(
            result.available_qualities,
            vec![Quality::P1080, Quality::P720]
        );
        // P1080 appears exactly once
        assert_eq!(
            result
                .available_qualities
                .iter()
                .filter(|&&q| q == Quality::P1080)
                .count(),
            1
        );
    }

    // 4.1 RED → GREEN — no video formats → empty available_qualities
    #[test]
    fn test_no_video_formats_empty_qualities() {
        let value = json!({
            "title": "No Video",
            "formats": [
                { "vcodec": "none", "ext": "m4a" },
                { "vcodec": "none", "ext": "opus" }
            ]
        });

        let result = parse_probe_json(&value).expect("should succeed");
        assert!(result.available_qualities.is_empty());
    }

    // 4.1 RED → GREEN — playlist JSON returns Err (not panic)
    #[test]
    fn test_playlist_json_no_panic() {
        let value = json!({
            "_type": "playlist",
            "title": "My Playlist",
            "entries": []
        });

        let result = parse_probe_json(&value);
        // Must not panic; may be Ok or Err
        match result {
            Ok(_) | Err(_) => {} // either is fine — just no panic
        }
    }

    // thumbnails[] array → pick the widest entry (not the top-level thumbnail)
    #[test]
    fn picks_widest_thumbnail_from_array() {
        let value = json!({
            "title": "Thumb Test",
            "thumbnail": "https://example.com/small.jpg",
            "thumbnails": [
                { "url": "https://example.com/120.jpg", "width": 120 },
                { "url": "https://example.com/1280.jpg", "width": 1280 },
                { "url": "https://example.com/640.jpg", "width": 640 }
            ],
            "formats": []
        });

        let result = parse_probe_json(&value).expect("should succeed");
        assert_eq!(
            result.thumbnail_url,
            Some("https://example.com/1280.jpg".to_string()),
            "must pick the widest thumbnail from the array"
        );
    }

    // No thumbnails array → fall back to the top-level thumbnail field
    #[test]
    fn falls_back_to_top_level_thumbnail() {
        let value = json!({
            "title": "Fallback Test",
            "thumbnail": "https://example.com/only.jpg",
            "formats": []
        });

        let result = parse_probe_json(&value).expect("should succeed");
        assert_eq!(
            result.thumbnail_url,
            Some("https://example.com/only.jpg".to_string())
        );
    }

    // ── Integration test (requires real yt-dlp binary) ──────────────────────

    /// INTEGRATION / MANUAL: spawns real yt-dlp.
    /// Run with: `cargo test -- --ignored`
    #[test]
    #[ignore]
    fn test_integration_real_ytdlp() {
        let path =
            crate::infrastructure::binary_probe::resolve_binary_path("yt-dlp", "YT_DLP_PATH")
                .expect("yt-dlp must be installed and on PATH");

        let probe = YtDlpProbe::new(path);
        let url =
            MediaUrl::parse("https://www.youtube.com/watch?v=jNQXAC9IVRw").expect("valid URL");

        let result = probe.probe(&url);
        assert!(result.is_ok(), "Expected Ok(MediaInfo), got: {result:?}");
        let info = result.unwrap();
        assert!(!info.title.is_empty(), "title must not be empty");
    }

    /// Assert that the ACTUAL probe arg builder always includes --no-playlist.
    /// Exercises `build_probe_args` (the same function the real probe calls),
    /// so a regression that drops --no-playlist would fail here.
    #[test]
    fn test_no_playlist_in_probe_args() {
        let url = MediaUrl::parse("https://www.youtube.com/watch?v=abc&list=PLxyz").unwrap();
        let args = build_probe_args(&url);
        assert!(
            args.contains(&"--no-playlist".to_string()),
            "--no-playlist must be present in probe args"
        );
        assert!(
            args.contains(&"-J".to_string()),
            "-J must be present in probe args"
        );
        assert_eq!(
            args.last().unwrap(),
            url.as_str(),
            "URL must be the last probe arg"
        );
    }
}
