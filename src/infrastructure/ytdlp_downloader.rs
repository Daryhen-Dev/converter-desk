use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::application::download_service::DownloadError;
use crate::application::ports::MediaDownloader;

const FORBIDDEN_ERROR: &str = "HTTP Error 403: Forbidden";
const EXTRACTOR_ARGS: &str = "--extractor-args";
const ANDROID_PLAYER_CLIENT: &str = "youtube:player_client=android";

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

        match run_ytdlp(&self.binary_path, &args, on_line) {
            Err(error) if should_retry_with_android_client(&args, &error) => {
                let retry_args = with_android_player_client(args);
                run_ytdlp(&self.binary_path, &retry_args, on_line)
            }
            result => result,
        }
    }
}

/// Execute yt-dlp, stream stdout to `on_line`, and convert process failures to
/// the downloader error type.
fn run_ytdlp(
    binary_path: &Path,
    args: &[String],
    on_line: &dyn Fn(&str),
) -> Result<(), DownloadError> {
    let mut child = std::process::Command::new(binary_path)
        .args(args)
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
                Ok(line) => on_line(&line),
                Err(_) => break, // pipe closed or read error — stop streaming
            }
        }
    }

    let status = child
        .wait()
        .map_err(|e| DownloadError::Failed(format!("failed to wait for yt-dlp: {e}")))?;

    // Join the stderr drain thread to collect what yt-dlp reported.
    let stderr_lines = stderr_join
        .map(|handle| handle.join().unwrap_or_default())
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

fn should_retry_with_android_client(args: &[String], error: &DownloadError) -> bool {
    is_mp3_extraction(args)
        && !args.iter().any(|arg| arg == EXTRACTOR_ARGS)
        && matches!(error, DownloadError::Failed(message) if message.contains(FORBIDDEN_ERROR))
}

fn is_mp3_extraction(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--extract-audio")
        && args
            .windows(2)
            .any(|arguments| arguments[0] == "--audio-format" && arguments[1] == "mp3")
}

fn with_android_player_client(mut args: Vec<String>) -> Vec<String> {
    if args.is_empty() || args.iter().any(|arg| arg == EXTRACTOR_ARGS) {
        return args;
    }

    let url = args.pop().expect("args is known to be non-empty");
    args.push(EXTRACTOR_ARGS.to_string());
    args.push(ANDROID_PLAYER_CLIENT.to_string());
    args.push(url);
    args
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
        Some(path) => {
            let mut output = Vec::with_capacity(args.len() + 2);
            output.push("--ffmpeg-location".to_string());
            output.push(path.to_string_lossy().into_owned());
            output.extend(args);
            output
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
    if let Some(error) = lines.iter().rev().find(|line| line.contains("ERROR")) {
        return error.trim().to_string();
    }
    lines
        .iter()
        .rev()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
        .unwrap_or_default()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{
        should_retry_with_android_client, with_android_player_client, with_ffmpeg_location,
        YtDlpDownloader,
    };
    use crate::application::download_service::DownloadError;
    use crate::application::ports::MediaDownloader;
    use std::path::Path;

    fn mp3_args() -> Vec<String> {
        vec![
            "--extract-audio".to_string(),
            "--audio-format".to_string(),
            "mp3".to_string(),
            "https://example.com/video".to_string(),
        ]
    }

    fn forbidden_error() -> DownloadError {
        DownloadError::Failed("yt-dlp failed: HTTP Error 403: Forbidden".to_string())
    }

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

    #[test]
    fn retries_only_mp3_extraction_forbidden_errors() {
        assert!(should_retry_with_android_client(
            &mp3_args(),
            &forbidden_error()
        ));
    }

    #[test]
    fn does_not_retry_video_downloads() {
        let video_args = vec![
            "--format".to_string(),
            "bestvideo+bestaudio".to_string(),
            "https://example.com/video".to_string(),
        ];
        assert!(!should_retry_with_android_client(
            &video_args,
            &forbidden_error()
        ));
    }

    #[test]
    fn does_not_retry_other_errors() {
        let other_error = DownloadError::Failed("yt-dlp failed: network unavailable".to_string());
        assert!(!should_retry_with_android_client(&mp3_args(), &other_error));
    }

    #[test]
    fn does_not_retry_non_mp3_audio_extraction() {
        let mut non_mp3_args = mp3_args();
        non_mp3_args[2] = "m4a".to_string();
        assert!(!should_retry_with_android_client(
            &non_mp3_args,
            &forbidden_error()
        ));
    }

    #[test]
    fn android_player_client_is_inserted_immediately_before_the_url() {
        let args = with_android_player_client(mp3_args());
        assert_eq!(
            args,
            vec![
                "--extract-audio",
                "--audio-format",
                "mp3",
                "--extractor-args",
                "youtube:player_client=android",
                "https://example.com/video",
            ]
        );
    }

    #[test]
    fn existing_extractor_args_prevent_retry_and_duplicate_injection() {
        let mut args = mp3_args();
        args.splice(
            args.len() - 1..args.len() - 1,
            [
                "--extractor-args".to_string(),
                "youtube:player_client=web".to_string(),
            ],
        );

        assert!(!should_retry_with_android_client(&args, &forbidden_error()));
        assert_eq!(with_android_player_client(args.clone()), args);
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
