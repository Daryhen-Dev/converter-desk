use crate::domain::format::Format;
use crate::domain::media_url::MediaUrl;
use crate::domain::quality::Quality;

/// yt-dlp progress template — pipe-delimited percent|speed|eta.
const PROGRESS_TEMPLATE: &str =
    "%(progress._percent_str)s|%(progress._speed_str)s|%(progress._eta_str)s";

/// Build the yt-dlp argument vector for the given format and URL.
///
/// Guarantees:
/// - `--no-playlist` always present
/// - `--newline` and `--progress-template <pipe-template>` always present
/// - Explicit output extension per format
/// - URL is the last element
/// - Returns `Vec<String>`, not a shell string
///
/// The `binary_path` is NOT included in the returned vector — use it as the
/// executable in `Command::new(binary_path).args(build_args(...))`.
pub fn build_args(
    format: Format,
    url: &MediaUrl,
    binary_path: &str,
    output_template: &str,
) -> Vec<String> {
    let (_binary, args) = build_command(format, url, binary_path, output_template);
    args
}

/// Returns `(binary_path, args_vector)`.
///
/// Separating binary from args allows callers to construct `Command::new(binary).args(args)`
/// without any shell interpolation.
pub fn build_command(
    format: Format,
    url: &MediaUrl,
    binary_path: &str,
    output_template: &str,
) -> (String, Vec<String>) {
    let mut args: Vec<String> = vec![
        // Always present — prevents infinite radio-mix downloads (PLANNING §7.1)
        "--no-playlist".to_string(),
        // Progress output: one line per update, pipe-delimited fields (PLANNING §7.3)
        "--newline".to_string(),
        "--progress-template".to_string(),
        PROGRESS_TEMPLATE.to_string(),
        // Output template
        "-o".to_string(),
        output_template.to_string(),
    ];

    // Format-specific flags (PLANNING §7.6 — set final extension explicitly)
    match format {
        Format::Video { quality } => {
            // Best video + audio with optional height constraint, remux to MP4
            args.push("-f".to_string());
            args.push(quality_to_format_selector(quality));
            args.push("--merge-output-format".to_string());
            args.push("mp4".to_string());
        }
        Format::AudioMp3 => {
            // Extract audio only, encode to MP3 — avoids name.webm.mp3
            args.push("--extract-audio".to_string());
            args.push("--audio-format".to_string());
            args.push("mp3".to_string());
        }
    }

    // URL must be the last argument (PLANNING §7.2)
    args.push(url.as_str().to_string());

    (binary_path.to_string(), args)
}

/// Map a `Quality` variant to the yt-dlp `-f` format selector string.
///
/// - `Quality::Best` → unconstrained `bestvideo+bestaudio/best`
/// - All other variants → height-bounded `bestvideo[height<=N]+bestaudio/best[height<=N]`
pub fn quality_to_format_selector(q: Quality) -> String {
    match q {
        Quality::Best => "bestvideo+bestaudio/best".to_string(),
        other => {
            let h = other.height_cap();
            format!("bestvideo[height<={}]+bestaudio/best[height<={}]", h, h)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::format::Format;
    use crate::domain::media_url::MediaUrl;
    use crate::domain::quality::Quality;

    fn make_url(s: &str) -> MediaUrl {
        MediaUrl::parse(s).unwrap()
    }

    // ── Migrated tests (VideoHighest → Video{Best}, assertions unchanged) ──

    // 3.1 RED → GREEN — --no-playlist present for Video{Best}
    #[test]
    fn no_playlist_present_for_video_highest() {
        let url = make_url("https://example.com/video");
        let args = super::build_args(
            Format::Video { quality: Quality::Best },
            &url,
            "yt-dlp",
            "%(title)s.%(ext)s",
        );
        assert!(args.contains(&"--no-playlist".to_string()), "--no-playlist must be in args");
    }

    // 3.2 RED → GREEN — --no-playlist present for AudioMp3
    #[test]
    fn no_playlist_present_for_audio_mp3() {
        let url = make_url("https://example.com/audio");
        let args = super::build_args(Format::AudioMp3, &url, "yt-dlp", "%(title)s.%(ext)s");
        assert!(args.contains(&"--no-playlist".to_string()), "--no-playlist must be in args");
    }

    // 3.3 RED → GREEN — URL is last element; result has more than one element
    #[test]
    fn url_is_last_element() {
        let url = make_url("https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        let args = super::build_args(
            Format::Video { quality: Quality::Best },
            &url,
            "yt-dlp",
            "%(title)s.%(ext)s",
        );
        assert!(args.len() > 1, "args vector must have more than one element");
        assert_eq!(
            args.last().unwrap(),
            url.as_str(),
            "URL must be the last element"
        );
    }

    // 3.4 RED → GREEN — URL with & = ? produces discrete vector elements
    #[test]
    fn url_with_metacharacters_is_discrete_element() {
        let url = make_url("https://example.com/watch?v=abc&list=xyz&index=1");
        let args = super::build_args(
            Format::Video { quality: Quality::Best },
            &url,
            "yt-dlp",
            "%(title)s.%(ext)s",
        );
        let url_str = url.as_str();
        let matching: Vec<&String> = args.iter().filter(|a| a.contains("abc")).collect();
        assert_eq!(
            matching.len(),
            1,
            "URL must appear as exactly one element, not split or duplicated"
        );
        assert_eq!(matching[0], url_str);
    }

    // 3.5 RED → GREEN — Video{Best} includes --merge-output-format mp4
    #[test]
    fn video_highest_includes_mp4_output_format() {
        let url = make_url("https://example.com/video");
        let args = super::build_args(
            Format::Video { quality: Quality::Best },
            &url,
            "yt-dlp",
            "%(title)s.%(ext)s",
        );
        assert!(
            args.contains(&"--merge-output-format".to_string()),
            "Video must contain --merge-output-format"
        );
        let idx = args.iter().position(|a| a == "--merge-output-format").unwrap();
        assert_eq!(args[idx + 1], "mp4", "--merge-output-format must be followed by mp4");
    }

    // 3.5 RED → GREEN — AudioMp3 includes --extract-audio --audio-format mp3
    #[test]
    fn audio_mp3_includes_extract_audio_and_format() {
        let url = make_url("https://example.com/audio");
        let args = super::build_args(Format::AudioMp3, &url, "yt-dlp", "%(title)s.%(ext)s");
        assert!(
            args.contains(&"--extract-audio".to_string()),
            "AudioMp3 must contain --extract-audio"
        );
        assert!(
            args.contains(&"--audio-format".to_string()),
            "AudioMp3 must contain --audio-format"
        );
        let idx = args.iter().position(|a| a == "--audio-format").unwrap();
        assert_eq!(args[idx + 1], "mp3", "--audio-format must be followed by mp3");
    }

    // 3.6 RED → GREEN — --newline present; --progress-template present with pipe-delimited template
    #[test]
    fn progress_template_flags_are_present() {
        let url = make_url("https://example.com/video");
        let args = super::build_args(
            Format::Video { quality: Quality::Best },
            &url,
            "yt-dlp",
            "%(title)s.%(ext)s",
        );
        assert!(args.contains(&"--newline".to_string()), "--newline must be present");
        assert!(
            args.contains(&"--progress-template".to_string()),
            "--progress-template must be present"
        );
        let idx = args.iter().position(|a| a == "--progress-template").unwrap();
        let template = &args[idx + 1];
        assert!(template.contains('|'), "progress-template must be pipe-delimited, got: {template}");
        assert!(template.contains("percent"), "progress-template must include percent field");
        assert!(template.contains("speed"), "progress-template must include speed field");
        assert!(template.contains("eta"), "progress-template must include eta field");
    }

    // 3.7 RED → GREEN — explicit binary path returned as executable, not in args
    #[test]
    fn explicit_binary_path_is_respected() {
        let url = make_url("https://example.com/video");
        let (binary, args) = super::build_command(
            Format::Video { quality: Quality::Best },
            &url,
            "/usr/local/bin/yt-dlp",
            "%(title)s.%(ext)s",
        );
        assert_eq!(binary, "/usr/local/bin/yt-dlp");
        assert!(
            !args.contains(&"/usr/local/bin/yt-dlp".to_string()),
            "Binary path must not be in args vector"
        );
    }

    #[test]
    fn default_binary_is_yt_dlp() {
        let url = make_url("https://example.com/video");
        let (binary, _args) = super::build_command(
            Format::Video { quality: Quality::Best },
            &url,
            "yt-dlp",
            "%(title)s.%(ext)s",
        );
        assert_eq!(binary, "yt-dlp");
    }

    // 3.8 RED → GREEN — Video{Best} and AudioMp3 produce distinct flag sets
    #[test]
    fn video_highest_and_audio_mp3_produce_distinct_flag_sets() {
        let url = make_url("https://example.com/video");
        let video_args = super::build_args(
            Format::Video { quality: Quality::Best },
            &url,
            "yt-dlp",
            "%(title)s.%(ext)s",
        );
        let audio_args =
            super::build_args(Format::AudioMp3, &url, "yt-dlp", "%(title)s.%(ext)s");
        assert_ne!(
            video_args, audio_args,
            "Video and AudioMp3 must produce distinct argument vectors"
        );
    }

    // ── NEW tests for quality-aware selectors ──

    // 2.1 RED — Video{Best} parity: same -f selector as former VideoHighest
    #[test]
    fn test_video_best_parity() {
        let url = make_url("https://example.com/video");
        let args = super::build_args(
            Format::Video { quality: Quality::Best },
            &url,
            "yt-dlp",
            "%(title)s.%(ext)s",
        );
        let f_idx = args.iter().position(|a| a == "-f").expect("-f must be present");
        assert_eq!(
            args[f_idx + 1],
            "bestvideo+bestaudio/best",
            "Video{{Best}} must produce unconstrained selector"
        );
    }

    // 2.1 RED — P1080 selector
    #[test]
    fn test_p1080_selector() {
        let url = make_url("https://example.com/video");
        let args = super::build_args(
            Format::Video { quality: Quality::P1080 },
            &url,
            "yt-dlp",
            "%(title)s.%(ext)s",
        );
        let f_idx = args.iter().position(|a| a == "-f").expect("-f must be present");
        assert_eq!(
            args[f_idx + 1],
            "bestvideo[height<=1080]+bestaudio/best[height<=1080]"
        );
    }

    // 2.1 RED — P720 selector
    #[test]
    fn test_p720_selector() {
        let url = make_url("https://example.com/video");
        let args = super::build_args(
            Format::Video { quality: Quality::P720 },
            &url,
            "yt-dlp",
            "%(title)s.%(ext)s",
        );
        let f_idx = args.iter().position(|a| a == "-f").expect("-f must be present");
        assert_eq!(
            args[f_idx + 1],
            "bestvideo[height<=720]+bestaudio/best[height<=720]"
        );
    }

    // 2.1 RED — invariants: all video variants preserve required flags + URL-last
    #[test]
    fn test_invariants_all_video_qualities() {
        let url = make_url("https://example.com/video");
        let variants = [
            Quality::Best,
            Quality::P2160,
            Quality::P1440,
            Quality::P1080,
            Quality::P720,
            Quality::P480,
            Quality::P360,
        ];
        for quality in variants {
            let args = super::build_args(
                Format::Video { quality },
                &url,
                "yt-dlp",
                "%(title)s.%(ext)s",
            );
            assert!(
                args.contains(&"--no-playlist".to_string()),
                "Quality {quality}: --no-playlist must be present"
            );
            assert!(
                args.contains(&"--newline".to_string()),
                "Quality {quality}: --newline must be present"
            );
            assert!(
                args.contains(&"--progress-template".to_string()),
                "Quality {quality}: --progress-template must be present"
            );
            assert!(
                args.contains(&"--merge-output-format".to_string()),
                "Quality {quality}: --merge-output-format must be present"
            );
            let merge_idx = args
                .iter()
                .position(|a| a == "--merge-output-format")
                .unwrap();
            assert_eq!(
                args[merge_idx + 1], "mp4",
                "Quality {quality}: --merge-output-format must be followed by mp4"
            );
            assert_eq!(
                args.last().unwrap(),
                url.as_str(),
                "Quality {quality}: URL must be last"
            );
        }
    }
}
