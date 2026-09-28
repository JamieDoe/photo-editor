use serde::{Deserialize, Serialize};

/// Display render quality. Export is a separate path that always renders from a
/// full-resolution decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum PreviewQuality {
    /// Very cheap; library grids.
    Thumbnail,
    /// While a control is being dragged: bounded resolution, latency first.
    Interactive,
    /// When interaction settles: up to the viewport's device resolution.
    Detail,
}

/// Resolution limits per quality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QualityLimits {
    pub thumbnail_long_edge: u32,
    pub interactive_max_long_edge: u32,
}

impl Default for QualityLimits {
    fn default() -> Self {
        Self {
            thumbnail_long_edge: 256,
            interactive_max_long_edge: 1280,
        }
    }
}

impl PreviewQuality {
    /// Long edge (pixels) to aim for, given the viewport's requested long edge.
    pub fn target_long_edge(self, requested: u32, limits: QualityLimits) -> u32 {
        let requested = requested.max(1);
        match self {
            Self::Thumbnail => limits.thumbnail_long_edge,
            Self::Interactive => requested.min(limits.interactive_max_long_edge),
            Self::Detail => requested,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_follow_quality() {
        let l = QualityLimits::default();
        assert_eq!(PreviewQuality::Thumbnail.target_long_edge(4000, l), 256);
        assert_eq!(PreviewQuality::Interactive.target_long_edge(4000, l), 1280);
        assert_eq!(PreviewQuality::Interactive.target_long_edge(800, l), 800);
        assert_eq!(PreviewQuality::Detail.target_long_edge(4000, l), 4000);
        assert_eq!(PreviewQuality::Detail.target_long_edge(0, l), 1);
    }
}
