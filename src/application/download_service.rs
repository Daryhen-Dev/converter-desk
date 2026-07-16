use thiserror::Error;

use crate::application::ports::{MediaDownloader, ProgressSink};
use crate::domain::job::DownloadJob;
use crate::infrastructure::arg_builder;
use crate::infrastructure::progress_parser::{self, ParsedLine};

/// Errors that can occur during download orchestration.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DownloadError {
    #[error("download failed: {0}")]
    Failed(String),

    #[error("binary not found: {0}")]
    BinaryNotFound(String),

    #[error("invalid argument: {0}")]
    InvalidArgument(String),
}

/// Orchestrates a download by delegating to a `MediaDownloader` port implementation.
///
/// Generic over `D: MediaDownloader` so the real yt-dlp adapter and test fakes
/// are interchangeable without dynamic dispatch at the service level.
pub struct DownloadService<D: MediaDownloader> {
    downloader: D,
}

impl<D: MediaDownloader> DownloadService<D> {
    /// Create a new service with the given downloader adapter.
    pub fn new(downloader: D) -> Self {
        Self { downloader }
    }

    /// Execute a download job, emitting progress events to `sink`.
    ///
    /// 1. Builds the yt-dlp argument vector via `arg_builder`.
    /// 2. Calls the downloader port with a line callback.
    /// 3. Each line is parsed and forwarded to `sink` as `Progress` or `Stage`.
    pub fn execute(
        &self,
        job: &DownloadJob,
        sink: &dyn ProgressSink,
    ) -> Result<(), DownloadError> {
        let (binary, args) = arg_builder::build_command(
            job.format,
            &job.url,
            "yt-dlp",
            &job.output_path,
        );

        self.downloader.download(&binary, args, &|line| {
            match progress_parser::parse_line(line) {
                ParsedLine::Progress(p) => sink.on_progress(p),
                ParsedLine::StageChange(s) => sink.on_stage(s),
                ParsedLine::Ignored => {}
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{DownloadError, DownloadService};
    use crate::application::ports::{MediaDownloader, ProgressSink};
    use crate::domain::format::Format;
    use crate::domain::job::{DownloadJob, Progress, Stage};
    use crate::domain::media_url::MediaUrl;

    fn make_job() -> DownloadJob {
        DownloadJob {
            url: MediaUrl::parse("https://example.com/video").unwrap(),
            format: Format::Video { quality: crate::domain::quality::Quality::Best },
            output_path: "%(title)s.%(ext)s".to_string(),
        }
    }

    // --- Fakes ---

    struct FakeDownloader;
    impl MediaDownloader for FakeDownloader {
        fn download(
            &self,
            _binary: &str,
            _args: Vec<String>,
            _on_line: &dyn Fn(&str),
        ) -> Result<(), DownloadError> {
            Ok(())
        }
    }

    struct FakeDownloaderWithProgress;
    impl MediaDownloader for FakeDownloaderWithProgress {
        fn download(
            &self,
            _binary: &str,
            _args: Vec<String>,
            on_line: &dyn Fn(&str),
        ) -> Result<(), DownloadError> {
            on_line("10.0%|1MiB/s|00:30");
            on_line("50.0%|2MiB/s|00:15");
            on_line("100.0%|3MiB/s|00:00");
            Ok(())
        }
    }

    struct FailingDownloader;
    impl MediaDownloader for FailingDownloader {
        fn download(
            &self,
            _binary: &str,
            _args: Vec<String>,
            _on_line: &dyn Fn(&str),
        ) -> Result<(), DownloadError> {
            Err(DownloadError::Failed("network error".to_string()))
        }
    }

    struct NoopSink;
    impl ProgressSink for NoopSink {
        fn on_progress(&self, _p: Progress) {}
        fn on_stage(&self, _s: Stage) {}
    }

    // Counting sink — uses std::sync::Mutex so it satisfies Send
    use std::sync::Mutex;
    struct CountingSink {
        progress_count: Mutex<u32>,
        stage_count: Mutex<u32>,
    }
    impl CountingSink {
        fn new() -> Self {
            Self {
                progress_count: Mutex::new(0),
                stage_count: Mutex::new(0),
            }
        }
        fn progress_count(&self) -> u32 {
            *self.progress_count.lock().unwrap()
        }
        fn stage_count(&self) -> u32 {
            *self.stage_count.lock().unwrap()
        }
    }
    impl ProgressSink for CountingSink {
        fn on_progress(&self, _p: Progress) {
            *self.progress_count.lock().unwrap() += 1;
        }
        fn on_stage(&self, _s: Stage) {
            *self.stage_count.lock().unwrap() += 1;
        }
    }

    // 4.3 RED → GREEN — service runs to completion with fake adapter
    #[test]
    fn service_runs_to_completion_with_fake_adapter() {
        let service = DownloadService::new(FakeDownloader);
        let result = service.execute(&make_job(), &NoopSink);
        assert!(result.is_ok(), "Expected Ok(()), got {result:?}");
    }

    // 4.4 RED → GREEN — FakeDownloader emits 3 progress lines → sink receives 3 on_progress calls
    #[test]
    fn service_propagates_progress_events() {
        let service = DownloadService::new(FakeDownloaderWithProgress);
        let sink = CountingSink::new();
        let _ = service.execute(&make_job(), &sink);
        assert_eq!(
            sink.progress_count(),
            3,
            "Expected 3 progress events, got {}",
            sink.progress_count()
        );
    }

    // Triangulation: stage events are also propagated
    #[test]
    fn service_propagates_stage_events() {
        struct FakeWithStage;
        impl MediaDownloader for FakeWithStage {
            fn download(
                &self,
                _binary: &str,
                _args: Vec<String>,
                on_line: &dyn Fn(&str),
            ) -> Result<(), DownloadError> {
                on_line("50.0%|2MiB/s|00:10");
                on_line("[Merger] Merging formats into \"output.mp4\"");
                Ok(())
            }
        }
        let service = DownloadService::new(FakeWithStage);
        let sink = CountingSink::new();
        let _ = service.execute(&make_job(), &sink);
        assert_eq!(sink.progress_count(), 1, "Expected 1 progress event");
        assert_eq!(sink.stage_count(), 1, "Expected 1 stage event");
    }

    // 4.5 RED → GREEN — FakeDownloader returns Err → service returns Err
    #[test]
    fn service_propagates_adapter_errors() {
        let service = DownloadService::new(FailingDownloader);
        let result = service.execute(&make_job(), &NoopSink);
        assert!(result.is_err(), "Expected Err, got Ok");
        assert_eq!(
            result.unwrap_err(),
            DownloadError::Failed("network error".to_string())
        );
    }
}
