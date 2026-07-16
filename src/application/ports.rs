use crate::domain::job::{Progress, Stage};
use crate::domain::media_info::MediaInfo;
use crate::domain::media_url::MediaUrl;

/// A sink that receives progress events during a download.
///
/// Implementations must be `Send` to allow use across thread boundaries.
/// Uses `&self` receivers so the sink can be shared as `&dyn ProgressSink`
/// without requiring ownership transfer.
pub trait ProgressSink: Send {
    /// Called when a new progress update is available.
    fn on_progress(&self, progress: Progress);
    /// Called when the download transitions to a new stage.
    fn on_stage(&self, stage: Stage);
}

/// Port for executing a download.
///
/// The adapter receives the yt-dlp argument vector and calls `on_line` for
/// each line of stdout, allowing the service to parse progress without
/// depending on any concrete process type.
pub trait MediaDownloader: Send + Sync {
    fn download(
        &self,
        binary: &str,
        args: Vec<String>,
        on_line: &dyn Fn(&str),
    ) -> Result<(), crate::application::download_service::DownloadError>;
}

/// Port for detecting whether a required external binary is available.
///
/// Does NOT import `std::process` in the trait definition itself.
pub trait BinaryProbe: Send + Sync {
    /// Check that `binary_name` is available and return its version string.
    fn check_available(
        &self,
        binary_name: &str,
    ) -> Result<String, crate::application::download_service::DownloadError>;
}

// ─── ProbeError ──────────────────────────────────────────────────────────────

/// Errors that can occur when probing a media URL for metadata.
#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    #[error("probe failed: {0}")]
    Failed(String),
    #[error("unsupported site or private video: {0}")]
    Unsupported(String),
}

// ─── MediaProbe ──────────────────────────────────────────────────────────────

/// Port for probing a media URL to retrieve its metadata.
///
/// Implementations spawn `yt-dlp --no-playlist -J <url>` and map the JSON
/// output to a `MediaInfo` domain struct.
///
/// `Send + Sync` bounds allow use from multiple threads.
pub trait MediaProbe: Send + Sync {
    fn probe(&self, url: &MediaUrl) -> Result<MediaInfo, ProbeError>;
}
