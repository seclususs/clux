// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

const PACE_MIN_US: u64 = 30_000_000;
const PACE_MAX_US: u64 = 900_000_000;
const DRIFT_LIMIT: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Hold,
    Apply,
    Reapply,
    Surrender,
}

pub const fn judge(current: u32, floor: u32, applied: bool, drifts: u8) -> Verdict {
    if current <= floor {
        Verdict::Hold
    } else if !applied {
        Verdict::Apply
    } else if drifts.saturating_add(1) >= DRIFT_LIMIT {
        Verdict::Surrender
    } else {
        Verdict::Reapply
    }
}

pub fn adopt(reached: u32, level: Option<u32>) -> u32 {
    level.map_or(reached, |value| reached.max(value))
}

#[derive(Clone, Copy, Debug)]
pub struct Pace {
    interval_us: u64,
}

impl Pace {
    pub const fn new() -> Self {
        Self {
            interval_us: PACE_MIN_US,
        }
    }

    pub fn next_us(&mut self, drifted: bool) -> u64 {
        let current = if drifted {
            PACE_MIN_US
        } else {
            self.interval_us
        };

        self.interval_us = current.saturating_mul(2).min(PACE_MAX_US);
        current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quiet_node_is_left_alone() {
        assert_eq!(judge(0, 0, false, 0), Verdict::Hold, "already at the floor");
        assert_eq!(judge(0, 0, true, 2), Verdict::Hold, "still at the floor");
        assert_eq!(
            judge(1, 5, false, 0),
            Verdict::Hold,
            "below the floor is never raised"
        );
    }

    #[test]
    fn a_noisy_node_is_silenced_once() {
        assert_eq!(judge(7, 0, false, 0), Verdict::Apply, "first silence");
    }

    #[test]
    fn a_node_that_comes_back_is_silenced_again() {
        assert_eq!(judge(7, 0, true, 0), Verdict::Reapply, "first drift");
        assert_eq!(judge(7, 0, true, 1), Verdict::Reapply, "second drift");
    }

    #[test]
    fn a_node_that_keeps_coming_back_is_given_up() {
        assert_eq!(judge(7, 0, true, 2), Verdict::Surrender, "third drift");
        assert_eq!(
            judge(7, 0, true, u8::MAX),
            Verdict::Surrender,
            "saturated count"
        );
    }

    #[test]
    fn a_clamped_write_raises_the_effective_floor() {
        assert_eq!(adopt(0, Some(4)), 4, "kernel clamped 0 up to its minimum");
        assert_eq!(
            judge(4, adopt(0, Some(4)), true, 0),
            Verdict::Hold,
            "no endless rewrites"
        );
    }

    #[test]
    fn the_effective_floor_never_goes_down() {
        assert_eq!(adopt(4, Some(0)), 4, "floor only ratchets up");
        assert_eq!(adopt(64, Some(64)), 64, "exact write keeps the floor");
        assert_eq!(adopt(7, None), 7, "unreadable node keeps the floor");
    }

    #[test]
    fn pace_backs_off_to_the_cap() {
        let mut pace = Pace::new();
        let waits: [u64; 8] = core::array::from_fn(|_| pace.next_us(false));
        assert_eq!(
            waits,
            [
                30_000_000,
                60_000_000,
                120_000_000,
                240_000_000,
                480_000_000,
                900_000_000,
                900_000_000,
                900_000_000
            ],
            "doubling until the cap"
        );
    }

    #[test]
    fn pace_returns_to_fast_checks_after_a_drift() {
        let mut pace = Pace::new();
        for _ in 0..6 {
            let _ = pace.next_us(false);
        }
        assert_eq!(
            pace.next_us(true),
            30_000_000,
            "drift restarts the schedule"
        );
        assert_eq!(pace.next_us(false), 60_000_000, "then backs off again");
    }
}
