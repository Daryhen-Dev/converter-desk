#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    VideoHighest,
    AudioMp3,
}

#[cfg(test)]
mod tests {
    use super::Format;

    // 2.1 RED → GREEN — Format::VideoHighest and Format::AudioMp3 are representable,
    // PartialEq/Clone/Copy derive
    #[test]
    fn format_video_highest_is_representable() {
        let f = Format::VideoHighest;
        assert_eq!(f, Format::VideoHighest);
    }

    #[test]
    fn format_audio_mp3_is_representable() {
        let f = Format::AudioMp3;
        assert_eq!(f, Format::AudioMp3);
    }

    #[test]
    fn format_variants_are_distinct() {
        assert_ne!(Format::VideoHighest, Format::AudioMp3);
    }

    #[test]
    fn format_is_copyable() {
        let original = Format::VideoHighest;
        let copy = original; // Copy semantics
        assert_eq!(original, copy);
    }

    #[test]
    fn format_is_cloneable() {
        let original = Format::AudioMp3;
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }
}
