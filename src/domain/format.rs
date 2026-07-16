use crate::domain::quality::Quality;

/// Output format selected by the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Video with a specific quality constraint.
    /// `Quality::Best` is the unconstrained form (equivalent to former `VideoHighest`).
    Video { quality: Quality },
    /// Audio extracted and encoded to MP3.
    AudioMp3,
}

#[cfg(test)]
mod tests {
    use super::Format;
    use crate::domain::quality::Quality;

    // Migrated format representability tests.
    #[test]
    fn format_video_best_is_representable() {
        let f = Format::Video { quality: Quality::Best };
        assert_eq!(f, Format::Video { quality: Quality::Best });
    }

    #[test]
    fn format_audio_mp3_is_representable() {
        let f = Format::AudioMp3;
        assert_eq!(f, Format::AudioMp3);
    }

    #[test]
    fn format_variants_are_distinct() {
        assert_ne!(
            Format::Video { quality: Quality::Best },
            Format::AudioMp3
        );
    }

    #[test]
    fn format_is_copyable() {
        let original = Format::Video { quality: Quality::Best };
        let copy = original;
        assert_eq!(original, copy);
    }
}
