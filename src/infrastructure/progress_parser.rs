use crate::domain::job::{Progress, Stage};

/// The result of parsing a single line from yt-dlp stdout.
#[derive(Debug, Clone, PartialEq)]
pub enum ParsedLine {
    /// A progress update with percent, speed, and ETA.
    Progress(Progress),
    /// A stage transition (e.g. [Merger], [ExtractAudio] → Processing).
    StageChange(Stage),
    /// Line not recognized or irrelevant — callers should skip it.
    Ignored,
}

/// Parse a single line of yt-dlp stdout output.
///
/// - Pipe-delimited `percent|speed|eta` lines → `ParsedLine::Progress`
/// - Lines starting with `[Merger]` or `[ExtractAudio]` → `ParsedLine::StageChange(Processing)`
/// - Everything else (including empty, partial, or unrecognized) → `ParsedLine::Ignored`
///
/// This function MUST NOT panic on any input.
pub fn parse_line(line: &str) -> ParsedLine {
    let trimmed = line.trim();

    // Detect processing-stage marker lines first (before pipe-split attempt)
    if trimmed.starts_with("[Merger]") || trimmed.starts_with("[ExtractAudio]") {
        return ParsedLine::StageChange(Stage::Processing);
    }

    // Attempt pipe-delimited progress parse: percent|speed|eta
    let parts: Vec<&str> = trimmed.splitn(3, '|').collect();
    if parts.len() == 3 {
        let percent_str = parts[0].trim_end_matches('%').trim();
        if let Ok(percent) = percent_str.parse::<f32>() {
            let speed = parts[1].trim().to_string();
            let eta = parts[2].trim().to_string();
            return ParsedLine::Progress(Progress {
                percent,
                speed,
                eta,
            });
        }
    }

    ParsedLine::Ignored
}

#[cfg(test)]
mod tests {
    use super::{parse_line, ParsedLine};
    use crate::domain::job::Stage;

    // 3.10 RED → GREEN — "42.5%|1.20MiB/s|00:42" → ParsedLine::Progress
    #[test]
    fn parses_well_formed_progress_line() {
        let result = parse_line("42.5%|1.20MiB/s|00:42");
        match result {
            ParsedLine::Progress(p) => {
                assert!(
                    (p.percent - 42.5).abs() < 0.01,
                    "percent must be 42.5, got {}",
                    p.percent
                );
                assert_eq!(p.speed, "1.20MiB/s");
                assert_eq!(p.eta, "00:42");
            }
            other => panic!("Expected ParsedLine::Progress, got {other:?}"),
        }
    }

    // Triangulation: 0% progress
    #[test]
    fn parses_zero_percent_progress() {
        let result = parse_line("0.0%|0KiB/s|--:--");
        match result {
            ParsedLine::Progress(p) => {
                assert!((p.percent - 0.0).abs() < 0.01);
                assert_eq!(p.speed, "0KiB/s");
                assert_eq!(p.eta, "--:--");
            }
            other => panic!("Expected ParsedLine::Progress, got {other:?}"),
        }
    }

    // Triangulation: 100% progress
    #[test]
    fn parses_complete_progress_line() {
        let result = parse_line("100.0%|3.50MiB/s|00:00");
        match result {
            ParsedLine::Progress(p) => {
                assert!((p.percent - 100.0).abs() < 0.01);
            }
            other => panic!("Expected ParsedLine::Progress, got {other:?}"),
        }
    }

    // 3.11 RED → GREEN — "[Merger] ..." → StageChange(Processing)
    #[test]
    fn merger_line_sets_processing_stage() {
        let result = parse_line("[Merger] Merging formats into \"output.mp4\"");
        assert_eq!(result, ParsedLine::StageChange(Stage::Processing));
    }

    // 3.12 RED → GREEN — "[ExtractAudio] ..." → StageChange(Processing)
    #[test]
    fn extract_audio_line_sets_processing_stage() {
        let result = parse_line("[ExtractAudio] Destination: output.mp3");
        assert_eq!(result, ParsedLine::StageChange(Stage::Processing));
    }

    // 3.13 RED → GREEN — malformed lines → Ignored; no panic
    #[test]
    fn empty_string_returns_ignored() {
        assert_eq!(parse_line(""), ParsedLine::Ignored);
    }

    #[test]
    fn partial_line_one_field_returns_ignored() {
        assert_eq!(parse_line("42.5%"), ParsedLine::Ignored);
    }

    #[test]
    fn unrecognized_download_line_returns_ignored() {
        assert_eq!(
            parse_line("[download] Destination: output.mp4"),
            ParsedLine::Ignored
        );
    }

    // Triangulation: two-field line (missing eta) → Ignored
    #[test]
    fn two_field_line_returns_ignored() {
        // splitn(3, '|') on "42.5%|1MiB/s" gives 2 parts, not 3 → Ignored
        assert_eq!(parse_line("42.5%|1MiB/s"), ParsedLine::Ignored);
    }

    // Triangulation: non-numeric percent → Ignored (not a panic)
    #[test]
    fn non_numeric_percent_returns_ignored() {
        assert_eq!(parse_line("N/A%|0KiB/s|--:--"), ParsedLine::Ignored);
    }
}
