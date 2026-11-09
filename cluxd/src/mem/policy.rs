// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use crate::automata::cadence::Cadence;
use crate::automata::fixed::at;
use crate::automata::ladder::Ladder;

const STRAIN_ENTER: [u32; 4] = [0, 40, 120, 300];
const STRAIN_LEAVE: [u32; 4] = [0, 25, 80, 220];
const STRAIN_HOLD_US: u64 = 3_000_000;
const PACE_MS: [u32; 4] = [250, 500, 2000, 5000];
const PACE_CEILING: [usize; 4] = [3, 2, 1, 0];
const PACE_PATIENCE: u32 = 2;

#[derive(Clone, Copy, Debug)]
pub struct Decision {
    pub strain: u32,
    pub next_us: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct Policy {
    strain: Ladder<4>,
    cadence: Cadence<4>,
}

impl Policy {
    pub const fn new() -> Self {
        Self {
            strain: Ladder::new(STRAIN_ENTER, STRAIN_LEAVE, STRAIN_HOLD_US),
            cadence: Cadence::new(PACE_MS, PACE_PATIENCE),
        }
    }

    pub fn interval_us(&self) -> u64 {
        self.cadence.interval_us()
    }

    pub fn step(&mut self, level: u32, now_us: u64, urgent: bool) -> Decision {
        let _ = self.strain.step(level, now_us);

        if urgent {
            self.cadence.boost();
        }

        let rung = self.strain.rung();

        Decision {
            strain: u32::try_from(rung).unwrap_or(u32::MAX),
            next_us: self.cadence.next_us(at(&PACE_CEILING, rung)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: u64 = 1_000_000;

    #[test]
    fn calm_memory_stays_calm_and_is_polled_slowly() {
        let mut policy = Policy::new();
        let decision = policy.step(0, 0, false);
        assert_eq!(decision.strain, 0, "no stall means no strain");
        assert_eq!(decision.next_us, 5_000_000, "calm memory is polled slowly");
    }

    #[test]
    fn rising_stall_climbs_at_once() {
        let mut policy = Policy::new();
        assert_eq!(policy.step(30, 0, false).strain, 0, "below the first gate");
        assert_eq!(policy.step(50, S, false).strain, 1, "light");
        assert_eq!(policy.step(130, 2 * S, false).strain, 2, "tight");
        assert_eq!(policy.step(320, 3 * S, false).strain, 3, "critical");
    }

    #[test]
    fn a_spike_skips_the_rungs_in_between() {
        let mut policy = Policy::new();
        assert_eq!(policy.step(400, 0, false).strain, 3, "straight to critical");
    }

    #[test]
    fn strain_is_released_only_after_the_hold() {
        let mut policy = Policy::new();
        assert_eq!(policy.step(150, 0, false).strain, 2, "tight");
        assert_eq!(policy.step(0, S, false).strain, 2, "still held");
        assert_eq!(policy.step(0, 2 * S, false).strain, 2, "hold not over");
        assert_eq!(policy.step(0, 5 * S, false).strain, 0, "released");
    }

    #[test]
    fn the_band_between_the_gates_keeps_the_rung() {
        let mut policy = Policy::new();
        assert_eq!(policy.step(150, 0, false).strain, 2, "tight");
        for tick in 1..10 {
            assert_eq!(
                policy.step(100, tick * S, false).strain,
                2,
                "inside the hysteresis band"
            );
        }
    }

    #[test]
    fn an_event_is_answered_at_the_fastest_pace() {
        let mut policy = Policy::new();
        let decision = policy.step(0, 0, true);
        assert_eq!(decision.next_us, 250_000, "event boosts the cadence");
    }

    #[test]
    fn pressure_is_tracked_faster_than_calm() {
        let mut policy = Policy::new();
        let calm = policy.step(0, 0, false).next_us;
        let strained = policy.step(400, S, false).next_us;
        assert!(strained < calm, "strain shortens the polling interval");
    }
}
