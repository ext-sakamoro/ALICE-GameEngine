//! Frame-row playback for 2D sprites: a row is a list of frame numbers with a
//! duration for each shown frame. Integer milliseconds, so the same inputs give
//! the same frames on every platform.

use std::collections::BTreeMap;

/// What happens at the end of a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowMode {
    /// Start again from the first shown frame.
    Loop,
    /// Stay on the last shown frame and report [`RowEvent::Finished`] once.
    Once,
}

/// One action's frames. `order[i]` is shown for `ms[i]`. A frame number may
/// repeat, which is how a row holds a frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteRow {
    pub order: Vec<u16>,
    pub ms: Vec<u32>,
    pub mode: RowMode,
}

/// Why a row was refused or a name was not found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpriteAnimError {
    EmptyRow,
    LengthMismatch { order: usize, ms: usize },
    ZeroDuration { index: usize },
    UnknownRow(String),
    ZeroStep,
    NothingPlaying,
}

/// Time left to walk once whole cycles are taken off, starting at a frame shown for
/// `here` ms in a row of `total` ms.
const fn skip_whole_cycles(elapsed: u64, here: u64, total: u64) -> u64 {
    if elapsed >= here + total {
        here + (elapsed - here) % total
    } else {
        elapsed
    }
}

/// Which way the figure faces when frames follow distance. Frames always run
/// forward along the facing, whichever sign the position has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Facing {
    Right,
    Left,
}

/// Reported by [`SpriteAnimator::advance`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowEvent {
    Finished,
}

/// Plays one row at a time.
#[derive(Debug, Clone, Default)]
pub struct SpriteAnimator {
    rows: BTreeMap<String, SpriteRow>,
    current: Option<String>,
    index: usize,
    elapsed: u64,
    finished: bool,
}

impl SpriteAnimator {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds or replaces a row.
    ///
    /// # Errors
    /// Refuses an empty row, differing lengths, or a zero duration.
    pub fn add_row(&mut self, name: &str, row: SpriteRow) -> Result<(), SpriteAnimError> {
        if row.order.is_empty() {
            return Err(SpriteAnimError::EmptyRow);
        }
        if row.order.len() != row.ms.len() {
            return Err(SpriteAnimError::LengthMismatch {
                order: row.order.len(),
                ms: row.ms.len(),
            });
        }
        if let Some(index) = row.ms.iter().position(|&m| m == 0) {
            return Err(SpriteAnimError::ZeroDuration { index });
        }
        self.rows.insert(name.to_string(), row);
        if self.current.as_deref() == Some(name) {
            self.index = 0;
            self.elapsed = 0;
            self.finished = false;
        }
        Ok(())
    }

    /// Starts a row. Playing the current row again changes nothing.
    ///
    /// # Errors
    /// Refuses a name that was not added.
    pub fn play(&mut self, name: &str) -> Result<(), SpriteAnimError> {
        if !self.rows.contains_key(name) {
            return Err(SpriteAnimError::UnknownRow(name.to_string()));
        }
        if self.current.as_deref() != Some(name) {
            self.current = Some(name.to_string());
            self.index = 0;
            self.elapsed = 0;
            self.finished = false;
        }
        Ok(())
    }

    /// Moves time forward. Returns [`RowEvent::Finished`] on the call that
    /// reaches the end of a [`RowMode::Once`] row, and never again for that play.
    pub fn advance(&mut self, ms: u32) -> Option<RowEvent> {
        let row = self.rows.get(self.current.as_deref()?)?;
        if self.finished {
            return None;
        }
        self.elapsed += u64::from(ms);
        if row.mode == RowMode::Loop {
            // Whole cycles change nothing, so a long pause costs no extra steps.
            let total: u64 = row.ms.iter().map(|&m| u64::from(m)).sum();
            let here = u64::from(row.ms[self.index]);
            self.elapsed = skip_whole_cycles(self.elapsed, here, total);
        }
        loop {
            let here = u64::from(row.ms[self.index]);
            if self.elapsed < here {
                return None;
            }
            if self.index + 1 < row.order.len() {
                self.elapsed -= here;
                self.index += 1;
            } else if row.mode == RowMode::Loop {
                self.elapsed -= here;
                self.index = 0;
            } else {
                self.finished = true;
                return Some(RowEvent::Finished);
            }
        }
    }

    /// Chooses the shown frame from the distance walked instead of from time,
    /// so a planted foot stays on its ground point at any speed. `x` is the
    /// position in whole units and `step` the units one frame moves the planted
    /// foot back. Returns the position to draw the figure at: `x` rounded down
    /// to a whole number of steps. Draw there, not at `x`.
    ///
    /// # Errors
    /// Refuses a zero step, or a call before the first [`Self::play`].
    pub fn seek_by_distance(
        &mut self,
        x: i64,
        step: u32,
        facing: Facing,
    ) -> Result<i64, SpriteAnimError> {
        if step == 0 {
            return Err(SpriteAnimError::ZeroStep);
        }
        let row = self
            .current
            .as_deref()
            .and_then(|name| self.rows.get(name))
            .ok_or(SpriteAnimError::NothingPlaying)?;
        let step = i64::from(step);
        let n = x.div_euclid(step);
        let along = match facing {
            Facing::Right => i128::from(n),
            Facing::Left => -i128::from(n),
        };
        // rem_euclid is below the row length, which came from a Vec.
        self.index = along.rem_euclid(row.order.len() as i128) as usize;
        self.elapsed = 0;
        self.finished = false;
        // Rounding down can step past i64::MIN only when x is within one step of it.
        Ok(n.checked_mul(step).unwrap_or(x))
    }

    /// The frame number to draw, or `None` before the first [`Self::play`].
    #[must_use]
    pub fn frame(&self) -> Option<u16> {
        self.rows
            .get(self.current.as_deref()?)
            .map(|r| r.order[self.index])
    }

    /// The position within the row's shown frames.
    #[must_use]
    pub const fn position(&self) -> usize {
        self.index
    }

    /// The row being played.
    #[must_use]
    pub fn current(&self) -> Option<&str> {
        self.current.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(order: &[u16], ms: &[u32], mode: RowMode) -> SpriteRow {
        SpriteRow {
            order: order.to_vec(),
            ms: ms.to_vec(),
            mode,
        }
    }
    fn anim(r: SpriteRow) -> SpriteAnimator {
        let mut a = SpriteAnimator::new();
        a.add_row("r", r).unwrap();
        a.play("r").unwrap();
        a
    }

    #[test]
    fn frame_changes_exactly_at_each_boundary() {
        let mut a = anim(row(&[5, 6, 7], &[100, 50, 20], RowMode::Loop));
        assert_eq!(a.frame(), Some(5));
        a.advance(99);
        assert_eq!(a.frame(), Some(5));
        a.advance(1);
        assert_eq!(a.frame(), Some(6));
        a.advance(49);
        assert_eq!(a.frame(), Some(6));
        a.advance(1);
        assert_eq!(a.frame(), Some(7));
    }

    #[test]
    fn one_advance_crosses_several_frames() {
        let mut a = anim(row(&[1, 2, 3, 4], &[10, 10, 10, 10], RowMode::Once));
        a.advance(25);
        assert_eq!(a.frame(), Some(3));
        assert_eq!(a.position(), 2);
    }

    #[test]
    fn loop_wraps_and_keeps_the_remainder() {
        let mut a = anim(row(&[1, 2], &[10, 30], RowMode::Loop));
        a.advance(45);
        assert_eq!(a.frame(), Some(1));
        a.advance(4);
        assert_eq!(a.frame(), Some(1));
        a.advance(1);
        assert_eq!(a.frame(), Some(2));
    }

    #[test]
    fn large_advance_on_loop_equals_small_steps() {
        let mut big = anim(row(&[1, 2, 3], &[7, 11, 13], RowMode::Loop));
        let mut small = big.clone();
        big.advance(100_003);
        for _ in 0..100_003 {
            small.advance(1);
        }
        assert_eq!(big.frame(), small.frame());
        assert_eq!(big.position(), small.position());
    }

    #[test]
    fn once_stops_on_last_and_reports_finished_one_time() {
        let mut a = anim(row(&[1, 2], &[10, 10], RowMode::Once));
        assert_eq!(a.advance(19), None);
        assert_eq!(a.advance(1), Some(RowEvent::Finished));
        assert_eq!(a.frame(), Some(2));
        assert_eq!(a.advance(1000), None);
        assert_eq!(a.frame(), Some(2));
    }

    #[test]
    fn holds_repeat_a_frame_number() {
        let mut a = anim(row(&[0, 0, 1], &[10, 10, 10], RowMode::Loop));
        a.advance(15);
        assert_eq!(a.frame(), Some(0));
        assert_eq!(a.position(), 1);
        a.advance(5);
        assert_eq!(a.frame(), Some(1));
    }

    #[test]
    fn playing_the_same_row_does_not_restart() {
        let mut a = anim(row(&[1, 2], &[10, 10], RowMode::Loop));
        a.advance(12);
        a.play("r").unwrap();
        assert_eq!(a.frame(), Some(2));
    }

    #[test]
    fn playing_another_row_restarts_and_clears_finished() {
        let mut a = anim(row(&[1, 2], &[10, 10], RowMode::Once));
        a.add_row("s", row(&[8, 9], &[5, 5], RowMode::Once))
            .unwrap();
        assert_eq!(a.advance(20), Some(RowEvent::Finished));
        a.play("s").unwrap();
        assert_eq!(a.frame(), Some(8));
        assert_eq!(a.current(), Some("s"));
        assert_eq!(a.advance(10), Some(RowEvent::Finished));
        a.play("r").unwrap();
        assert_eq!(a.frame(), Some(1));
        assert_eq!(a.advance(20), Some(RowEvent::Finished));
    }

    #[test]
    fn bad_rows_are_refused() {
        let mut a = SpriteAnimator::new();
        assert_eq!(
            a.add_row("e", row(&[], &[], RowMode::Loop)),
            Err(SpriteAnimError::EmptyRow)
        );
        assert_eq!(
            a.add_row("l", row(&[1], &[1, 2], RowMode::Loop)),
            Err(SpriteAnimError::LengthMismatch { order: 1, ms: 2 })
        );
        assert_eq!(
            a.add_row("z", row(&[1, 2], &[1, 0], RowMode::Loop)),
            Err(SpriteAnimError::ZeroDuration { index: 1 })
        );
        assert_eq!(
            a.play("e"),
            Err(SpriteAnimError::UnknownRow("e".to_string()))
        );
    }

    #[test]
    fn replacing_the_current_row_with_a_shorter_one_stays_in_range() {
        let mut a = anim(row(&[1, 2, 3], &[10, 10, 10], RowMode::Loop));
        a.advance(25);
        a.add_row("r", row(&[9], &[10], RowMode::Loop)).unwrap();
        assert_eq!(a.frame(), Some(9));
        a.advance(1);
        assert_eq!(a.frame(), Some(9));
    }

    // Oracle for distance frames: a foot table that moves back `step` per frame
    // (stance is a straight line by construction, written out here, not read from
    // the code) must keep one world point while that foot is planted.
    fn stance_foot(position: usize, step: i64) -> i64 {
        -(position as i64) * step
    }

    fn walk() -> SpriteAnimator {
        anim(row(&[0, 1, 2, 3, 4, 5, 6, 7], &[50; 8], RowMode::Loop))
    }

    #[test]
    fn planted_foot_keeps_its_world_point_walking_right() {
        let (mut a, step) = (walk(), 6);
        // Stance is positions 0..=4 of each cycle. Start one whole cycle's walk from x = 0.
        let mut points = Vec::new();
        for x in 0..(4 * 6) {
            let snapped = a.seek_by_distance(x, 6, Facing::Right).unwrap();
            points.push(snapped + stance_foot(a.position(), step));
        }
        assert!(points.iter().all(|&p| p == 0), "{points:?}");
    }

    #[test]
    fn planted_foot_keeps_its_world_point_walking_left() {
        let (mut a, step) = (walk(), 6);
        // Mirrored figure: the foot's offset changes sign, and the walk goes down in x.
        // x = 0 is the first unit of a new step, so the walk starts from the step below.
        let mut points = Vec::new();
        for x in (-(4 * 6)..0).rev() {
            let snapped = a.seek_by_distance(x, 6, Facing::Left).unwrap();
            points.push(snapped - stance_foot(a.position(), step));
        }
        assert!(points.windows(2).all(|w| w[0] == w[1]), "{points:?}");
    }

    #[test]
    fn frames_follow_distance_and_wrap() {
        let mut a = walk();
        for (x, position, snapped) in [(0, 0, 0), (5, 0, 0), (6, 1, 6), (47, 7, 42), (48, 0, 48)] {
            assert_eq!(
                a.seek_by_distance(x, 6, Facing::Right),
                Ok(snapped),
                "x {x}"
            );
            assert_eq!(a.position(), position, "x {x}");
        }
    }

    #[test]
    fn negative_positions_round_down_not_toward_zero() {
        let mut a = walk();
        assert_eq!(a.seek_by_distance(-1, 6, Facing::Right), Ok(-6));
        assert_eq!(a.position(), 7);
        assert_eq!(a.seek_by_distance(-1, 6, Facing::Left), Ok(-6));
        assert_eq!(a.position(), 1);
    }

    #[test]
    fn standing_still_keeps_the_frame() {
        let mut a = walk();
        let first = a.seek_by_distance(100, 6, Facing::Right).unwrap();
        let position = a.position();
        assert_eq!(a.seek_by_distance(100, 6, Facing::Right), Ok(first));
        assert_eq!(a.position(), position);
    }

    #[test]
    fn distance_refuses_a_zero_step_and_a_missing_row() {
        let mut a = walk();
        a.seek_by_distance(30, 6, Facing::Right).unwrap();
        let position = a.position();
        assert_eq!(
            a.seek_by_distance(30, 0, Facing::Right),
            Err(SpriteAnimError::ZeroStep)
        );
        assert_eq!(a.position(), position, "a refused call changes nothing");
        assert_eq!(
            SpriteAnimator::new().seek_by_distance(0, 1, Facing::Right),
            Err(SpriteAnimError::NothingPlaying)
        );
    }

    // Extremes: the position stays in range and the result is the documented value,
    // not merely free of a panic. i64::MIN with step 1 is the one negation that overflows i64.
    #[test]
    fn distance_at_the_ends_of_the_range() {
        let mut a = walk();
        assert_eq!(a.seek_by_distance(i64::MIN, 1, Facing::Left), Ok(i64::MIN));
        // 2^63 is a multiple of 8, so Left and Right both land on position 0.
        assert_eq!(a.position(), 0);
        assert_eq!(a.seek_by_distance(i64::MAX, 1, Facing::Right), Ok(i64::MAX));
        assert_eq!(a.position(), 7);
        // Rounding down would leave the range: the position itself is returned.
        assert_eq!(a.seek_by_distance(i64::MIN, 6, Facing::Right), Ok(i64::MIN));
        assert!(a.position() < 8);
    }

    // 2^63 mod 3 = 2 (2^odd mod 3 = 2), so a negation that wraps in i64 lands elsewhere.
    #[test]
    fn left_at_i64_min_uses_the_true_negation() {
        let mut a = anim(row(&[0, 1, 2], &[50; 3], RowMode::Loop));
        assert_eq!(a.seek_by_distance(i64::MIN, 1, Facing::Left), Ok(i64::MIN));
        assert_eq!(a.position(), 2);
    }

    #[test]
    fn seeking_drops_the_time_already_spent() {
        let mut a = walk();
        a.advance(30);
        a.seek_by_distance(0, 6, Facing::Right).unwrap();
        a.advance(30);
        assert_eq!(a.position(), 0, "30 + 30 would cross the 50 ms frame");
    }

    #[test]
    fn seeking_reopens_a_finished_row() {
        let mut a = anim(row(&[0, 1], &[50, 50], RowMode::Once));
        assert_eq!(a.advance(100), Some(RowEvent::Finished));
        a.seek_by_distance(0, 6, Facing::Right).unwrap();
        assert_eq!(a.advance(50), None);
        assert_eq!(a.position(), 1);
    }

    #[test]
    fn time_after_distance_starts_from_the_sought_frame() {
        let mut a = walk();
        a.seek_by_distance(12, 6, Facing::Right).unwrap();
        assert_eq!(a.position(), 2);
        a.advance(50);
        assert_eq!(a.position(), 3);
    }

    // Oracle for a long pause: frames walked one call at a time, 1 ms each.
    fn walked_position(ms: &[u32], total_ms: u32) -> usize {
        let order: Vec<u16> = (0..ms.len() as u16).collect();
        let mut a = anim(row(&order, ms, RowMode::Loop));
        for _ in 0..total_ms {
            a.advance(1);
        }
        a.position()
    }

    #[test]
    fn long_pause_lands_where_small_steps_land() {
        let ms = [50, 60, 70];
        for pause in [230, 1000, 1007, 5000] {
            let mut a = anim(row(&[0, 1, 2], &ms, RowMode::Loop));
            a.advance(pause);
            assert_eq!(a.position(), walked_position(&ms, pause), "pause {pause}");
        }
    }

    // A row that plays once has no cycles to skip: 160 ms covers 50 + 60, though
    // 160 is a whole number of cycles past the first frame.
    #[test]
    fn long_pause_finishes_a_row_that_plays_once() {
        let mut a = anim(row(&[0, 1], &[50, 60], RowMode::Once));
        assert_eq!(a.advance(160), Some(RowEvent::Finished));
    }

    // Worked by hand: 1000 - 50 = 950 = 5 * 180 + 50, so 50 + 50 is left.
    #[test]
    fn whole_cycles_are_taken_off_a_long_pause() {
        for (elapsed, here, total, left) in [
            (229, 50, 180, 229),
            (230, 50, 180, 50),
            (1000, 50, 180, 100),
            (u64::from(u32::MAX), 1, 2, 1),
        ] {
            assert_eq!(skip_whole_cycles(elapsed, here, total), left, "{elapsed}");
        }
    }

    #[test]
    fn before_play_there_is_no_frame_and_advance_is_inert() {
        let mut a = SpriteAnimator::new();
        assert_eq!(a.frame(), None);
        assert_eq!(a.advance(100), None);
        assert_eq!(a.current(), None);
    }
}
