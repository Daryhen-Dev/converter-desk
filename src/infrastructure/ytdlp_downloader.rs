use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::Stdio;

use crate::application::download_service::DownloadError;
use crate::application::ports::MediaDownloader;

/// Concrete adapter that spawns yt-dlp as a subprocess.
///
/// The binary path is resolved at construction time and stored here.
/// The `binary` parameter received from `DownloadService` is intentionally
/// ignored — this adapter is the authority on which executable to invoke.
/// This is a documented MVP smell: the `MediaDownloader` trait passes a
/// `binary` string that real adapters own independently.
pub struct YtDlpDownloader {
    binary_path: PathBuf,
}

impl YtDlpDownloader {
    /// Construct a downloader that will invoke `binary_path` directly.
    ///
    /// `binary_path` must point to the resolved yt-dlp executable (obtained
    /// from `resolve_binary_path`). It is NOT a shell command string.
    pub fn new(binary_path: PathBuf) -> Self {
        Self { binary_path }
    }
}

impl MediaDownloader for YtDlpDownloader {
    /// Spawn yt-dlp with the given argument vector, stream stdout line-by-line
    /// to `on_line`, then wait for the process to exit.
    ///
    /// # Notes
    /// - `_binary` is intentionally ignored; `self.binary_path` is used instead.
    /// - No shell (`sh`, `cmd`) is involved — only `Command::new(&self.binary_path)`.
    /// - Non-zero exit → `Err(DownloadError::Failed(...))`
    /// - Spawn error → `Err(DownloadError::Failed(...))`
    fn download(
        &self,
        _binary: &str,
        args: Vec<String>,
        on_line: &dyn Fn(&str),
    ) -> Result<(), DownloadError> {
        let mut child = std::process::Command::new(&self.binary_path)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| DownloadError::Failed(format!("failed to spawn yt-dlp: {e}")))?;

        // Stream stdout line by line to the callback.
        // `take()` moves the stdout handle out of `child` so we can still call `wait()`.
        if let Some(stdout) = child.stdout.take() {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(l) => on_line(&l),
                    Err(_) => break, // pipe closed or read error — stop streaming
                }
            }
        }

        let status = child
            .wait()
            .map_err(|e| DownloadError::Failed(format!("failed to wait for yt-dlp: {e}")))?;

        if status.success() {
            Ok(())
        } else {
            Err(DownloadError::Failed(format!(
                "yt-dlp exited with non-zero status: {status}"
            )))
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::YtDlpDownloader;
    use crate::application::ports::MediaDownloader;

    /// INTEGRATION / MANUAL: requires a real yt-dlp binary.
    /// Run with: `cargo test -- --ignored`
    /// Tested manually on Arch Linux and Windows before tagging releases.
    #[test]
    #[ignore]
    fn integration_successful_download_returns_ok() {
        // Resolve yt-dlp from the environment.
        let path =
            crate::infrastructure::binary_probe::resolve_binary_path("yt-dlp", "YT_DLP_PATH")
                .expect("yt-dlp must be installed and on PATH to run this test");

        let downloader = YtDlpDownloader::new(path);

        // A short publicly accessible audio clip (public domain, ~5s).
        // Replace with a reliable short URL if this one becomes unavailable.
        let url = "https://www.youtube.com/watch?v=jNQXAC9IVRw"; // "Me at the zoo" — 18s
        let args = vec![
            "--no-playlist".to_string(),
            "--simulate".to_string(), // Do not actually download; just verify the URL resolves.
            url.to_string(),
        ];

        let lines_received = std::cell::Cell::new(0usize);
        let result = downloader.download("yt-dlp", args, &|_line| {
            lines_received.set(lines_received.get() + 1);
        });

        assert!(
            result.is_ok(),
            "Expected Ok(()), got: {result:?}\n\
             Hint: ensure yt-dlp is installed (YT_DLP_PATH or PATH) and the URL is valid."
        );
    }

    /// INTEGRATION / MANUAL: verifies non-zero exit → Err.
    /// Run with: `cargo test -- --ignored`
    /// Tested manually on Arch Linux and Windows before tagging releases.
    #[test]
    #[ignore]
    fn integration_nonzero_exit_returns_err() {
        let path =
            crate::infrastructure::binary_probe::resolve_binary_path("yt-dlp", "YT_DLP_PATH")
                .expect("yt-dlp must be installed and on PATH to run this test");

        let downloader = YtDlpDownloader::new(path);

        // An intentionally invalid URL should cause yt-dlp to exit non-zero.
        let args = vec![
            "--no-playlist".to_string(),
            "https://this-url-is-completely-invalid-xyz-42.example".to_string(),
        ];

        let result = downloader.download("yt-dlp", args, &|_line| {});

        assert!(
            result.is_err(),
            "Expected Err for invalid URL, got Ok — yt-dlp may have exited 0 unexpectedly"
        );
    }
}
