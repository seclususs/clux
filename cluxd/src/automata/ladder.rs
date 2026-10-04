// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

#[derive(Clone, Copy, Debug)]
pub struct Ladder<const N: usize> {
    rung: usize,
    since: Option<u64>,
    enter: [u32; N],
    leave: [u32; N],
    hold_us: u64,
}

impl<const N: usize> Ladder<N> {
    pub const fn new(enter: [u32; N], leave: [u32; N], hold_us: u64) -> Self {
        Self {
            rung: 0,
            since: None,
            enter,
            leave,
            hold_us,
        }
    }

    pub const fn rung(&self) -> usize {
        self.rung
    }

    pub fn step(&mut self, input: u32, now_us: u64) -> bool {
        let mut up = self.rung;
        while self
            .enter
            .get(up.saturating_add(1))
            .is_some_and(|&gate| input >= gate)
        {
            up = up.saturating_add(1);
        }

        if up > self.rung {
            self.rung = up;
            self.since = None;
            return true;
        }

        let target = self.target(input);
        if target >= self.rung {
            self.since = None;
            return false;
        }

        let began = *self.since.get_or_insert(now_us);
        if now_us.saturating_sub(began) < self.hold_us {
            return false;
        }

        self.rung = target;
        self.since = None;
        true
    }

    fn target(&self, input: u32) -> usize {
        let mut rung = self.rung;
        while rung > 0 && self.leave.get(rung).is_some_and(|&gate| input < gate) {
            rung = rung.saturating_sub(1);
        }
        rung
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: u64 = 1_000_000;

    fn ladder() -> Ladder<4> {
        Ladder::new([0, 100, 300, 600], [0, 70, 240, 500], 2 * SECOND)
    }

    #[test]
    fn escalates_immediately_and_skips_rungs() {
        let mut l = ladder();
        assert!(l.step(350, 0));
        assert_eq!(l.rung(), 2);
        assert!(l.step(700, 1));
        assert_eq!(l.rung(), 3);
    }

    #[test]
    fn hysteresis_band_holds_rung() {
        let mut l = ladder();
        let _ = l.step(150, 0);
        assert_eq!(l.rung(), 1);
        for tick in 1..20 {
            assert!(!l.step(80, tick * SECOND));
        }
        assert_eq!(l.rung(), 1);
    }

    #[test]
    fn descends_only_after_hold_and_jumps_to_target() {
        let mut l = ladder();
        let _ = l.step(700, 0);
        assert!(!l.step(10, SECOND));
        assert!(!l.step(10, 2 * SECOND));
        assert!(l.step(10, 3 * SECOND));
        assert_eq!(l.rung(), 0);
    }

    #[test]
    fn interrupted_calm_restarts_hold() {
        let mut l = ladder();
        let _ = l.step(350, 0);
        assert!(!l.step(10, SECOND));
        assert!(!l.step(310, 2 * SECOND));
        assert!(!l.step(10, 3 * SECOND));
        assert!(!l.step(10, 4 * SECOND));
        assert!(l.step(10, 5 * SECOND));
    }

    #[test]
    fn partial_descent_stops_at_band() {
        let mut l = ladder();
        let _ = l.step(700, 0);
        let _ = l.step(260, SECOND);
        assert!(l.step(260, 3 * SECOND));
        assert_eq!(l.rung(), 2);
    }
}
