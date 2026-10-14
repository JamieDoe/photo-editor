//! Smooth drags on slower machines (PERFORMANCE §65). An interactive preview renders
//! from the pyramid level just sharp enough for the viewer; with a heavy edit
//! (clarity, noise reduction, grain…) on a machine with few cores that can take 50 ms
//! or more, and a drag then shows a frame every few display refreshes. While that is
//! so, drags render from the next level down (a quarter of the pixels, half as sharp)
//! and the detail render that follows a drag restores full sharpness.
//!
//! The cost of an edit is tracked as render time per megapixel, which hardly depends
//! on the level, so one render predicts the others.

/// The render time an interactive frame should fit in, leaving room in a 60 Hz frame
/// for the trip to the window.
const BUDGET_MS: f64 = 14.0;
/// Going back up a level only once the larger one would fit well within the budget,
/// so frames don't switch sharpness back and forth around it.
const STEP_UP_SHARE: f64 = 0.8;
/// Each render's weight in the running cost: high, as the edit changes the cost.
const WEIGHT: f64 = 0.5;
/// At most this many levels below the normal one (half as sharp, at most).
const MAX_STEPS: usize = 1;

#[derive(Debug, Default)]
pub(crate) struct DragPace {
    ms_per_megapixel: Option<f64>,
    steps: usize,
}

impl DragPace {
    /// Records a viewer render of `megapixels` that took `render_ms`.
    pub(crate) fn record(&mut self, render_ms: f64, megapixels: f64) {
        if megapixels <= 0.0 || !render_ms.is_finite() {
            return;
        }
        let sample = render_ms / megapixels;
        self.ms_per_megapixel = Some(match self.ms_per_megapixel {
            Some(cost) => cost + WEIGHT * (sample - cost),
            None => sample,
        });
    }

    /// How many levels below `normal` an interactive render should use, given each
    /// level's megapixels (`megapixels(i)`, `None` past the smallest).
    pub(crate) fn steps_down(
        &mut self,
        normal: usize,
        megapixels: impl Fn(usize) -> Option<f64>,
    ) -> usize {
        let Some(cost) = self.ms_per_megapixel else {
            return 0;
        };
        let predicted = |steps: usize| megapixels(normal + steps).map(|mp| mp * cost);
        let mut steps = self.steps.min(MAX_STEPS);
        while steps < MAX_STEPS
            && predicted(steps).is_some_and(|ms| ms > BUDGET_MS)
            && predicted(steps + 1).is_some()
        {
            steps += 1;
        }
        while steps > 0 && predicted(steps - 1).is_some_and(|ms| ms <= BUDGET_MS * STEP_UP_SHARE) {
            steps -= 1;
        }
        // A level that doesn't exist (a small photo) isn't used.
        while steps > 0 && predicted(steps).is_none() {
            steps -= 1;
        }
        self.steps = steps;
        steps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 24 MP photo's levels from the normal interactive one: 1.5, 0.38, 0.1 MP.
    fn levels(i: usize) -> Option<f64> {
        [6.1, 1.53, 0.383, 0.096].get(i).copied()
    }

    #[test]
    fn light_edits_keep_the_normal_level() {
        let mut pace = DragPace::default();
        assert_eq!(pace.steps_down(1, levels), 0, "nothing measured yet");
        pace.record(4.0, 1.53);
        assert_eq!(pace.steps_down(1, levels), 0);
    }

    #[test]
    fn heavy_edits_drop_one_level_and_come_back_with_margin() {
        let mut pace = DragPace::default();
        // 58 ms at 1.5 MP (38 ms/MP): down a level.
        pace.record(58.0, 1.53);
        assert_eq!(pace.steps_down(1, levels), 1);
        // Never more than one level, however slow.
        pace.record(400.0, 0.383);
        assert_eq!(pace.steps_down(1, levels), 1);
        // The edit gets lighter: 2.5 ms at the small level is 10 ms at the normal one,
        // within 80% of the budget, so back up.
        for _ in 0..12 {
            pace.record(2.5, 0.383);
        }
        assert_eq!(pace.steps_down(1, levels), 0);
    }

    #[test]
    fn hovering_near_the_budget_does_not_flip_levels() {
        let mut pace = DragPace::default();
        pace.record(16.0, 1.53); // over the budget: down
        assert_eq!(pace.steps_down(1, levels), 1);
        pace.record(13.0 * 0.383 / 1.53, 0.383); // 13 ms at full size: fits, not with margin
        assert_eq!(pace.steps_down(1, levels), 1, "stays down");
    }

    #[test]
    fn the_smallest_level_has_nowhere_to_go() {
        let mut pace = DragPace::default();
        pace.record(80.0, 0.096);
        assert_eq!(pace.steps_down(3, levels), 0);
    }
}
