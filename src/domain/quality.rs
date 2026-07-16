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

    /// Ordering key for descending sort: higher value = better / more prominent.
    ///
    /// `Best` maps to `u32::MAX` so it always sorts first in a descending sort,
    /// routing around the `height_cap` panic that guards against programmer error.
    pub fn sort_key(self) -> u32 {
        match self {
            Quality::Best => u32::MAX,
            other => other.height_cap(),
        }
    }

    /// Build the selectable list for the Quality ComboBox.
    ///
    /// - Deduplicates entries from `available`.
    /// - Sorts descending by `sort_key` (Best → u32::MAX always lands first).
    /// - Always includes `Quality::Best` exactly once at index 0.
    /// - Empty input → `vec![Quality::Best]`.
    pub fn selectable_list(available: &[Quality]) -> Vec<Quality> {
        use std::collections::HashSet;

        let mut set: HashSet<Quality> = available.iter().copied().collect();
        // Ensure Best is always present
        set.insert(Quality::Best);

        let mut list: Vec<Quality> = set.into_iter().collect();
        // Descending sort by sort_key — Best (u32::MAX) always lands at index 0
        list.sort_by_key(|q| std::cmp::Reverse(q.sort_key()));
        list
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

    // ── sort_key tests ────────────────────────────────────────────────────────

    #[test]
    fn sort_key_best_is_u32_max_no_panic() {
        assert_eq!(Quality::Best.sort_key(), u32::MAX);
    }

    #[test]
    fn sort_key_p2160_is_2160() {
        assert_eq!(Quality::P2160.sort_key(), 2160);
    }

    #[test]
    fn sort_key_p1080_is_1080() {
        assert_eq!(Quality::P1080.sort_key(), 1080);
    }

    #[test]
    fn sort_key_descending_order() {
        // Best > P2160 > P1440 > P1080 > P720 > P480 > P360
        let ordered = [
            Quality::Best,
            Quality::P2160,
            Quality::P1440,
            Quality::P1080,
            Quality::P720,
            Quality::P480,
            Quality::P360,
        ];
        for window in ordered.windows(2) {
            assert!(
                window[0].sort_key() > window[1].sort_key(),
                "{:?}.sort_key() should be > {:?}.sort_key()",
                window[0],
                window[1]
            );
        }
    }

    // ── selectable_list tests ─────────────────────────────────────────────────

    #[test]
    fn empty_returns_best_first() {
        let result = Quality::selectable_list(&[]);
        assert_eq!(result, vec![Quality::Best]);
    }

    #[test]
    fn dedup_and_sort_desc_best_first() {
        // Input: [P720, P1080, P720] — dedup → {P720, P1080}, sort desc → [P1080, P720]
        // Best prepended → [Best, P1080, P720]
        let result = Quality::selectable_list(&[Quality::P720, Quality::P1080, Quality::P720]);
        assert_eq!(result[0], Quality::Best, "Best must be at index 0");
        assert_eq!(result[1], Quality::P1080);
        assert_eq!(result[2], Quality::P720);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn already_sorted_preserved_best_first() {
        // Input: [P2160, P1080, P720, P480] → [Best, P2160, P1080, P720, P480]
        let result = Quality::selectable_list(&[
            Quality::P2160,
            Quality::P1080,
            Quality::P720,
            Quality::P480,
        ]);
        assert_eq!(result[0], Quality::Best);
        assert_eq!(result[1], Quality::P2160);
        assert_eq!(result[2], Quality::P1080);
        assert_eq!(result[3], Quality::P720);
        assert_eq!(result[4], Quality::P480);
        assert_eq!(result.len(), 5);
    }

    #[test]
    fn input_already_contains_best_no_duplicate() {
        // Input: [P1080, Best] — Best is in input, should appear exactly once at front
        let result = Quality::selectable_list(&[Quality::P1080, Quality::Best]);
        assert_eq!(result[0], Quality::Best, "Best must be at index 0");
        assert_eq!(result[1], Quality::P1080);
        assert_eq!(result.len(), 2, "Best must not be duplicated");
    }

    #[test]
    fn all_same_deduped() {
        // Input: [P720, P720, P720] → dedup to {P720} → [Best, P720]
        let result = Quality::selectable_list(&[Quality::P720, Quality::P720, Quality::P720]);
        assert_eq!(result[0], Quality::Best);
        assert_eq!(result[1], Quality::P720);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn single_element_no_best_produces_two_items() {
        let result = Quality::selectable_list(&[Quality::P1080]);
        assert_eq!(result, vec![Quality::Best, Quality::P1080]);
    }

    #[test]
    fn result_is_sorted_descending_by_sort_key() {
        let result = Quality::selectable_list(&[
            Quality::P480,
            Quality::P720,
            Quality::P1080,
            Quality::P2160,
        ]);
        // Should be [Best, P2160, P1080, P720, P480]
        for window in result.windows(2) {
            assert!(
                window[0].sort_key() >= window[1].sort_key(),
                "Result must be sorted descending: {:?} >= {:?}",
                window[0],
                window[1]
            );
        }
    }

    // ── from_height boundary tests ────────────────────────────────────────────

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
