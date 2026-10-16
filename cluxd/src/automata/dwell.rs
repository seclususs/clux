// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

#[derive(Clone, Copy, Debug)]
pub struct Dwell {
    held: u32,
    want: u32,
    since_us: u64,
    wrote_us: Option<u64>,
    settle_us: u64,
    gap_us: u64,
}

impl Dwell {
    pub const fn new(held: u32, settle_us: u64, gap_us: u64) -> Self {
        Self {
            held,
            want: held,
            since_us: 0,
            wrote_us: None,
            settle_us,
            gap_us,
        }
    }

    pub fn admit(&mut self, want: u32, now_us: u64) -> Option<u32> {
        if want != self.want {
            self.want = want;
            self.since_us = now_us;
        }

        if now_us.saturating_sub(self.since_us) < self.settle_us {
            return None;
        }

        if want != self.held {
            let spaced = self
                .wrote_us
                .is_none_or(|wrote| now_us.saturating_sub(wrote) >= self.gap_us);

            if !spaced {
                return None;
            }

            self.held = want;
            self.wrote_us = Some(now_us);
        }

        Some(want)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: u64 = 1_000_000;

    fn dwell() -> Dwell {
        Dwell::new(1000, 4 * S, 10 * S)
    }

    #[test]
    fn the_held_value_passes_through() {
        let mut d = dwell();
        assert_eq!(d.admit(1000, 100 * S), Some(1000));
        assert_eq!(d.admit(1000, 101 * S), Some(1000));
    }

    #[test]
    fn a_new_target_waits_until_it_is_steady() {
        let mut d = dwell();
        assert_eq!(d.admit(500, 100 * S), None);
        assert_eq!(d.admit(500, 103 * S), None);
        assert_eq!(d.admit(500, 104 * S), Some(500));
    }

    #[test]
    fn a_flapping_target_never_passes() {
        let mut d = dwell();
        for step in 0..20_u64 {
            let want = if step % 2 == 0 { 500 } else { 750 };
            assert_eq!(d.admit(want, (100 + step * 2) * S), None);
        }
    }

    #[test]
    fn a_target_that_reverts_before_settling_is_never_written() {
        let mut d = dwell();
        assert_eq!(d.admit(500, 100 * S), None);
        assert_eq!(d.admit(500, 103 * S), None);
        assert_eq!(d.admit(1000, 103 * S + 1), None);
        assert_eq!(d.admit(1000, 108 * S), Some(1000));
    }

    #[test]
    fn writes_are_spaced_apart() {
        let mut d = dwell();
        assert_eq!(d.admit(500, 100 * S), None);
        assert_eq!(d.admit(500, 104 * S), Some(500));
        assert_eq!(d.admit(1000, 105 * S), None);
        assert_eq!(d.admit(1000, 109 * S), None);
        assert_eq!(d.admit(1000, 113 * S), None);
        assert_eq!(d.admit(1000, 114 * S), Some(1000));
    }

    #[test]
    fn an_admitted_value_keeps_passing_so_failed_writes_retry() {
        let mut d = dwell();
        assert_eq!(d.admit(500, 100 * S), None);
        assert_eq!(d.admit(500, 104 * S), Some(500));
        assert_eq!(d.admit(500, 105 * S), Some(500));
        assert_eq!(d.admit(500, 106 * S), Some(500));
        assert_eq!(d.admit(1000, 106 * S + 1), None);
        assert_eq!(d.admit(1000, 110 * S + 2), None);
        assert_eq!(d.admit(1000, 114 * S), Some(1000));
    }

    #[test]
    fn a_clock_that_runs_backwards_cannot_admit() {
        let mut d = dwell();
        assert_eq!(d.admit(500, 100 * S), None);
        assert_eq!(d.admit(500, 0), None);
        assert_eq!(d.admit(500, u64::MAX), Some(500));
    }
}
