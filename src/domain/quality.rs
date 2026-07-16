/// Video quality level derived from stream height (pixels).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Quality {
    /// 2160p / 4K
    P2160,
    /// 1440p / 2K
    P1440,
    /// 1080p Full HD
    P1080,
    /// 720p HD
    P720,
    /// 480p SD
    P480,
    /// 360p and below
    P360,
    /// Unconstrained — let yt-dlp pick the best available
    Best,
}

impl Quality {
    /// Map a stream height (in pixels) to the appropriate quality bucket.
    ///
    /// Thresholds (inclusive lower bound):
    /// - h >= 2160 → P2160
    /// - h >= 1440 → P1440
    /// - h >= 1080 → P1080
    /// - h >= 720  → P720
    /// - h >= 480  → P480
    /// - h <  480  → P360
    pub fn from_height(h: u32) -> Self {
        match h {
            h if h >= 2160 => Quality::P2160,
            h if h >= 1440 => Quality::P1440,
            h if h >= 1080 => Quality::P1080,
            h if h >= 720 => Quality::P720,
            h if h >= 480 => Quality::P480,
            _ => Quality::P360,
        }
    }

    /// Return the numeric height ceiling for this quality level.
    ///
    /// Used by `quality_to_format_selector` in `arg_builder`.
    /// `Best` has no ceiling — callers handle it separately.
    pub fn height_cap(self) -> u32 {
        match self {
            Quality::P2160 => 2160,
            Quality::P1440 => 1440,
            Quality::P1080 => 1080,
            Quality::P720 => 720,
            Quality::P480 => 480,
            Quality::P360 => 360,
            Quality::Best => panic!("Best has no height cap"),
        }
    }
}

impl std::fmt::Display for Quality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Quality::P2160 => "2160p",
            Quality::P1440 => "1440p",
            Quality::P1080 => "1080p",
            Quality::P720 => "720p",
            Quality::P480 => "480p",
            Quality::P360 => "360p",
            Quality::Best => "Best",
        };
        write!(f, "{label}")
    }
}

#[cfg(test)]
mod tests {
    use super::Quality;

    // 1.1 RED — boundary tests for from_height
    // These tests are written BEFORE the implementation exists (todo! placeholder).

    #[test]
    fn from_height_zero_is_p360() {
        assert_eq!(Quality::from_height(0), Quality::P360);
    }

    #[test]
    fn from_height_479_is_p360() {
        assert_eq!(Quality::from_height(479), Quality::P360);
    }

    #[test]
    fn from_height_480_is_p480() {
        assert_eq!(Quality::from_height(480), Quality::P480);
    }

    #[test]
    fn from_height_719_is_p480() {
        assert_eq!(Quality::from_height(719), Quality::P480);
    }

    #[test]
    fn from_height_720_is_p720() {
        assert_eq!(Quality::from_height(720), Quality::P720);
    }

    #[test]
    fn from_height_1079_is_p720() {
        assert_eq!(Quality::from_height(1079), Quality::P720);
    }

    #[test]
    fn from_height_1080_is_p1080() {
        assert_eq!(Quality::from_height(1080), Quality::P1080);
    }

    #[test]
    fn from_height_1439_is_p1080() {
        assert_eq!(Quality::from_height(1439), Quality::P1080);
    }

    #[test]
    fn from_height_1440_is_p1440() {
        assert_eq!(Quality::from_height(1440), Quality::P1440);
    }

    #[test]
    fn from_height_2159_is_p1440() {
        assert_eq!(Quality::from_height(2159), Quality::P1440);
    }

    #[test]
    fn from_height_2160_is_p2160() {
        assert_eq!(Quality::from_height(2160), Quality::P2160);
    }

    #[test]
    fn from_height_9999_is_p2160() {
        assert_eq!(Quality::from_height(9999), Quality::P2160);
    }

    #[test]
    fn display_p1080() {
        assert_eq!(Quality::P1080.to_string(), "1080p");
    }

    #[test]
    fn display_best() {
        assert_eq!(Quality::Best.to_string(), "Best");
    }
}
