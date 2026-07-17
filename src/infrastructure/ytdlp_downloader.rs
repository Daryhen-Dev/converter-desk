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
    /// Optional explicit ffmpeg path. When set, passed to yt-dlp via
    /// `--ffmpeg-location` so muxing/transcoding uses this ffmpeg instead of
    /// relying on the ambient PATH (PLANNING §7.7). Enables portable bundles.
    ffmpeg_path: Option<PathBuf>,
}

impl YtDlpDownloader {
    /// Construct a downloader that will invoke `binary_path` directly.
    ///
    /// `binary_path` must point to the resolved yt-dlp executable (obtained
    /// from `resolve_binary_path`). It is NOT a shell command string.
    /// `ffmpeg_path`, when `Some`, is forwarded to yt-dlp as `--ffmpeg-location`.
    pub fn new(binary_path: PathBuf, ffmpeg_path: Option<PathBuf>) -> Self {
        Self {
            binary_path,
            ffmpeg_path,
        }
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
        // Inject an explicit ffmpeg location when known, so a bundled ffmpeg is
        // used regardless of PATH (PLANNING §7.7).
        let args = with_ffmpeg_location(args, self.ffmpeg_path.as_deref());

        let mut child = std::process::Command::new(&self.binary_path)
            .args(&args)
            // Give yt-dlp a valid (null) stdin. When the app is launched from a
            // GUI shortcut (no console), the parent's stdin handle is invalid;
            // inheriting it makes yt-dlp fail with "[Errno 22] Invalid argument"
            // on Windows. Setting it explicitly avoids that.
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| DownloadError::Failed(format!("failed to spawn yt-dlp: {e}")))?;

        // Drain stderr on a dedicated thread. If we only read stdout and yt-dlp
        // fills its stderr pipe buffer, the child blocks on write and we
        // deadlock — so stderr must be consumed concurrently. yt-dlp prints its
        // `ERROR:` diagnostics here, which we surface on a non-zero exit.
        let stderr_join = child.stderr.take().map(|stderr| {
            std::thread::spawn(move || {
                BufReader::new(stderr)
                    .lines()
                    .map_while(Result::ok)
                    .collect::<Vec<String>>()
            })
        });

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

        // Join the stderr drain thread to collect what yt-dlp reported.
        let stderr_lines = stderr_join
            .map(|h| h.join().unwrap_or_default())
            .unwrap_or_default();

        if status.success() {
            Ok(())
        } else {
            let detail = stderr_tail(&stderr_lines);
            Err(DownloadError::Failed(if detail.is_empty() {
                format!("yt-dlp exited with non-zero status: {status}")
            } else {
                format!("yt-dlp failed ({status}): {detail}")
            }))
        }
    }
}

/// Prepend an explicit `--ffmpeg-location <path>` to the yt-dlp args when a
/// bundled/resolved ffmpeg path is known, so yt-dlp uses that ffmpeg for
/// muxing/transcoding instead of relying on the ambient PATH (PLANNING §7.7).
///
/// When `ffmpeg_path` is `None` the args are returned unchanged.
pub(crate) fn with_ffmpeg_location(
    args: Vec<String>,
    ffmpeg_path: Option<&std::path::Path>,
) -> Vec<String> {
    match ffmpeg_path {
        Some(p) => {
            let mut out = Vec::with_capacity(args.len() + 2);
            out.push("--ffmpeg-location".to_string());
            out.push(p.to_string_lossy().into_owned());
            out.extend(args);
            out
        }
        None => args,
    }
}

/// Pick the most relevant line from yt-dlp's captured stderr for error reporting.
///
/// Prefers the last line containing `ERROR` (yt-dlp's own error marker);
/// otherwise falls back to the last non-empty line. Returns an empty string
/// when stderr produced nothing.
fn stderr_tail(lines: &[String]) -> String {
    if let Some(err) = lines.iter().rev().find(|l| l.contains("ERROR")) {
        return err.trim().to_string();
    }
    lines
        .iter()
        .rev()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim().to_string())
        .unwrap_or_default()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{with_ffmpeg_location, YtDlpDownloader};
    use crate::application::ports::MediaDownloader;
    use std::path::Path;

    // Pure unit test: --ffmpeg-location is prepended when a path is provided.
    #[test]
    fn ffmpeg_location_prepended_when_some() {
        let base = vec![
            "--no-playlist".to_string(),
            "https://example.com/v".to_string(),
        ];
        let out = with_ffmpeg_location(base, Some(Path::new("C:/tools/ffmpeg.exe")));
        assert_eq!(out[0], "--ffmpeg-location");
        assert_eq!(out[1], "C:/tools/ffmpeg.exe");
        // Original args preserved after the injected flag; URL stays last.
        assert_eq!(out.last().unwrap(), "https://example.com/v");
    }

    // Pure unit test: args unchanged when no ffmpeg path is known.
    #[test]
    fn ffmpeg_location_absent_when_none() {
        let base = vec!["--no-playlist".to_string(), "url".to_string()];
        let out = with_ffmpeg_location(base.clone(), None);
        assert_eq!(out, base);
    }

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

        let downloader = YtDlpDownloader::new(path, None);

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

        let downloader = YtDlpDownloader::new(path, None);

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
