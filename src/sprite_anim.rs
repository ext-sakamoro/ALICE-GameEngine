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
            if self.elapsed >= here + total {
                self.elapsed = here + (self.elapsed - here) % total;
            }
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

    #[test]
    fn before_play_there_is_no_frame_and_advance_is_inert() {
        let mut a = SpriteAnimator::new();
        assert_eq!(a.frame(), None);
        assert_eq!(a.advance(100), None);
        assert_eq!(a.current(), None);
    }
}
