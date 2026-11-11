// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use crate::automata::dwell::Dwell;
use crate::automata::fixed::{at, stretch};
use crate::automata::ladder::Ladder;
use crate::psi::Screen;

const NORMAL: u32 = 0;
const BURST: u32 = 1;
const SAVER: u32 = 2;
const SHARES: [[u32; 3]; 3] = [[1000, 1000, 1000], [500, 2000, 500], [2000, 500, 1000]];
const UP_FLOOR: u32 = 100;
const UP_CAP: u32 = 20_000;
const DOWN_FLOOR: u32 = 1000;
const DOWN_CAP: u32 = 100_000;
const BUSY_ENTER: [u32; 2] = [0, 150];
const BUSY_LEAVE: [u32; 2] = [0, 80];
const BUSY_HOLD_US: u64 = 2_000_000;
const GAP_US: u64 = 500_000;
const BURST_US: u64 = 500_000;
const NORMAL_US: u64 = 3_000_000;
const SAVER_US: u64 = 5_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Origin {
    pub up: u32,
    pub down: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    pub up: u32,
    pub down: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct Input {
    pub cpu: u32,
    pub screen: Screen,
    pub engaged: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Decision {
    pub regime: Option<u32>,
    pub next_us: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct Policy {
    busy: Ladder<2>,
    dwell: Dwell,
}

pub fn plan(origin: Origin, regime: u32, split: bool) -> Plan {
    let row = at(&SHARES, usize::try_from(regime).unwrap_or(0));
    let share = |column: usize| at(&row, column);

    if split {
        Plan {
            up: stretch(origin.up, share(0), UP_FLOOR, UP_CAP),
            down: stretch(origin.down, share(1), DOWN_FLOOR, DOWN_CAP),
        }
    } else {
        Plan {
            up: stretch(origin.up, share(2), UP_FLOOR, UP_CAP),
            down: 0,
        }
    }
}

const fn want(input: Input, busy: bool) -> u32 {
    if matches!(input.screen, Screen::Off) {
        SAVER
    } else if input.engaged || busy {
        BURST
    } else {
        NORMAL
    }
}

const fn pace(regime: u32) -> u64 {
    match regime {
        BURST => BURST_US,
        SAVER => SAVER_US,
        _ => NORMAL_US,
    }
}

impl Policy {
    pub const fn new() -> Self {
        Self {
            busy: Ladder::new(BUSY_ENTER, BUSY_LEAVE, BUSY_HOLD_US),
            dwell: Dwell::new(NORMAL, 0, GAP_US),
        }
    }

    pub fn step(&mut self, input: Input, now_us: u64) -> Decision {
        let _ = self.busy.step(input.cpu, now_us);

        let wanted = want(input, self.busy.rung() > 0);

        Decision {
            regime: self.dwell.admit(wanted, now_us),
            next_us: pace(wanted),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: u64 = 1_000_000;
    const SPLIT: Origin = Origin {
        up: 500,
        down: 20_000,
    };

    fn lit(cpu: u32, engaged: bool) -> Input {
        Input {
            cpu,
            screen: Screen::On,
            engaged,
        }
    }

    fn dark() -> Input {
        Input {
            cpu: 0,
            screen: Screen::Off,
            engaged: false,
        }
    }

    #[test]
    fn the_baseline_is_the_device_default() {
        assert_eq!(
            plan(SPLIT, NORMAL, true),
            Plan {
                up: 500,
                down: 20_000
            },
            "split limits"
        );
        assert_eq!(
            plan(Origin { up: 1000, down: 0 }, NORMAL, false),
            Plan { up: 1000, down: 0 },
            "single limit"
        );
    }

    #[test]
    fn a_burst_ramps_up_fast_and_holds_the_frequency() {
        assert_eq!(
            plan(SPLIT, BURST, true),
            Plan {
                up: 250,
                down: 40_000
            },
            "split limits"
        );
        assert_eq!(
            plan(Origin { up: 1000, down: 0 }, BURST, false),
            Plan { up: 500, down: 0 },
            "single limit"
        );
    }

    #[test]
    fn a_dark_screen_ramps_up_slowly_and_drops_early() {
        assert_eq!(
            plan(SPLIT, SAVER, true),
            Plan {
                up: 1000,
                down: 10_000
            },
            "split limits"
        );
        assert_eq!(
            plan(Origin { up: 1000, down: 0 }, SAVER, false),
            Plan { up: 1000, down: 0 },
            "a single limit cannot favour one direction"
        );
    }

    #[test]
    fn limits_stay_inside_their_bounds() {
        let fine = Origin {
            up: 150,
            down: 80_000,
        };
        let burst = plan(fine, BURST, true);
        assert_eq!(burst.up, 100, "up floor");
        assert_eq!(burst.down, 100_000, "down cap");
    }

    #[test]
    fn a_bound_never_moves_the_device_default_the_wrong_way() {
        let odd = Origin {
            up: 50,
            down: 150_000,
        };
        let burst = plan(odd, BURST, true);
        assert_eq!(burst.up, 50, "a floor above the default keeps the default");
        assert_eq!(burst.down, 150_000, "a cap below the default keeps it");
        assert_eq!(
            plan(odd, SAVER, true).down,
            75_000,
            "lowering is not held back by the cap"
        );
    }

    #[test]
    fn a_disabled_limit_stays_disabled() {
        let none = Origin { up: 0, down: 0 };
        for regime in [NORMAL, BURST, SAVER] {
            assert_eq!(plan(none, regime, true), Plan { up: 0, down: 0 }, "split");
            assert_eq!(plan(none, regime, false).up, 0, "single");
        }
    }

    #[test]
    fn a_dark_screen_outranks_touch_and_load() {
        let input = Input {
            cpu: 900,
            screen: Screen::Off,
            engaged: true,
        };
        assert_eq!(want(input, true), SAVER, "nobody is watching");
    }

    #[test]
    fn touch_or_load_means_a_burst_on_a_lit_screen() {
        assert_eq!(want(lit(0, true), false), BURST, "touch");
        assert_eq!(want(lit(0, false), true), BURST, "load");
        assert_eq!(want(lit(0, false), false), NORMAL, "calm");
    }

    #[test]
    fn a_burst_is_applied_without_waiting() {
        let mut policy = Policy::new();
        assert_eq!(
            policy.step(lit(0, true), 10 * S).regime,
            Some(BURST),
            "first touch"
        );
    }

    #[test]
    fn load_holds_the_burst_through_the_hysteresis_band() {
        let mut policy = Policy::new();
        assert_eq!(
            policy.step(lit(200, false), 10 * S).regime,
            Some(BURST),
            "busy"
        );
        assert_eq!(
            policy.step(lit(100, false), 12 * S).regime,
            Some(BURST),
            "inside the band"
        );
        assert_eq!(
            policy.step(lit(10, false), 13 * S).regime,
            Some(BURST),
            "hold not over"
        );
        assert_eq!(
            policy.step(lit(10, false), 16 * S).regime,
            Some(NORMAL),
            "released"
        );
    }

    #[test]
    fn regime_changes_are_spaced() {
        let mut policy = Policy::new();
        assert_eq!(
            policy.step(lit(0, true), 10 * S).regime,
            Some(BURST),
            "burst"
        );
        assert_eq!(
            policy
                .step(dark(), 10 * S + GAP_US.saturating_sub(1))
                .regime,
            None,
            "too soon"
        );
        assert_eq!(
            policy.step(dark(), 10 * S + GAP_US).regime,
            Some(SAVER),
            "spaced"
        );
    }

    #[test]
    fn the_pace_follows_the_regime() {
        let mut policy = Policy::new();
        assert_eq!(policy.step(lit(0, false), 0).next_us, NORMAL_US, "normal");
        assert_eq!(policy.step(lit(0, true), S).next_us, BURST_US, "burst");
        assert_eq!(policy.step(dark(), 2 * S).next_us, SAVER_US, "saver");
    }
}
