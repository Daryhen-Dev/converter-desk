use crate::domain::quality::Quality;

/// Metadata for a media URL, derived from yt-dlp's JSON probe output.
///
/// Pure domain type — no serde derives. JSON parsing lives in the
/// infrastructure adapter (`infrastructure/ytdlp_probe.rs`).
#[derive(Debug, Clone)]
pub struct MediaInfo {
    /// Video/audio title from the site metadata.
    pub title: String,
    /// Direct URL to the thumbnail image, if available.
    pub thumbnail_url: Option<String>,
    /// Duration in seconds, if available.
    pub duration_secs: Option<f64>,
    /// Uploader / channel name, if available.
    pub uploader: Option<String>,
    /// Available video quality levels, sorted descending, deduped.
    /// Derived from yt-dlp `formats[]` entries where `vcodec != "none"` and
    /// `height` is present. Audio-only formats are excluded.
    pub available_qualities: Vec<Quality>,
}
