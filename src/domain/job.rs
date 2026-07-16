use crate::domain::format::Format;
use crate::domain::media_url::MediaUrl;

/// Live progress data for a running download.
#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    /// Download percentage, 0.0–100.0.
    pub percent: f32,
    /// Human-readable speed (e.g. "1.20MiB/s").
    pub speed: String,
    /// Human-readable ETA (e.g. "00:42").
    pub eta: String,
}

/// The current processing stage of a download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage {
    /// Active network transfer — progress percent is meaningful.
    Downloading,
    /// Post-download mux/transcode ([Merger] or [ExtractAudio]) — percent not meaningful.
    Processing,
    /// Download and processing finished successfully.
    Complete,
    /// An error occurred.
    Error,
}

/// Lifecycle state of a download job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobStatus {
    /// Job created but not yet started.
    Pending,
    /// Job is actively running.
    Running,
    /// Job completed successfully.
    Done,
    /// Job failed; carries an error description.
    Failed(String),
}

/// A download job definition — immutable once created.
#[derive(Debug, Clone)]
pub struct DownloadJob {
    pub url: MediaUrl,
    pub format: Format,
    /// Output path or directory template.
    pub output_path: String,
}

#[cfg(test)]
mod tests {
    use super::{DownloadJob, JobStatus, Progress, Stage};
    use crate::domain::format::Format;
    use crate::domain::media_url::MediaUrl;

    // 2.7 RED → GREEN — Progress, Stage, JobStatus, DownloadJob
    #[test]
    fn progress_carries_percent_speed_eta() {
        let p = Progress {
            percent: 42.0,
            speed: "1.20MiB/s".to_string(),
            eta: "00:42".to_string(),
        };
        assert_eq!(p.percent, 42.0_f32);
        assert_eq!(p.speed, "1.20MiB/s");
        assert_eq!(p.eta, "00:42");
    }

    #[test]
    fn progress_zero_percent_is_valid() {
        let p = Progress {
            percent: 0.0,
            speed: "0KiB/s".to_string(),
            eta: "--:--".to_string(),
        };
        assert_eq!(p.percent, 0.0_f32);
    }

    #[test]
    fn stage_downloading_and_processing_are_distinct() {
        assert_ne!(Stage::Downloading, Stage::Processing);
    }

    #[test]
    fn stage_all_variants_are_representable() {
        let _d = Stage::Downloading;
        let _p = Stage::Processing;
        let _c = Stage::Complete;
        let _e = Stage::Error;
        assert_ne!(Stage::Complete, Stage::Error);
    }

    #[test]
    fn job_status_has_all_four_states() {
        assert_ne!(JobStatus::Pending, JobStatus::Running);
        assert_ne!(JobStatus::Running, JobStatus::Done);
        let failed = JobStatus::Failed("network error".to_string());
        assert_ne!(failed, JobStatus::Done);
    }

    #[test]
    fn download_job_is_constructible() {
        let url = MediaUrl::parse("https://example.com/video").unwrap();
        let job = DownloadJob {
            url,
            format: Format::Video {
                quality: crate::domain::quality::Quality::Best,
            },
            output_path: "/tmp/video.mp4".to_string(),
        };
        assert_eq!(
            job.format,
            Format::Video {
                quality: crate::domain::quality::Quality::Best
            }
        );
        assert_eq!(job.output_path, "/tmp/video.mp4");
    }
}
