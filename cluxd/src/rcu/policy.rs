// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use crate::automata::dwell::Dwell;
use crate::psi::Screen;

const NORMAL: u32 = 0;
const EXPEDITE: u32 = 1;
const RELAX: u32 = 2;
const GAP_US: u64 = 1_000_000;
const ACTIVE_US: u64 = 500_000;
const LIT_US: u64 = 3_000_000;
const DARK_US: u64 = 5_000_000;

#[derive(Clone, Copy, Debug)]
pub struct Input {
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
    dwell: Dwell,
}

pub const fn target(origin: u32, regime: u32) -> u32 {
    match regime {
        EXPEDITE => 1,
        RELAX => 0,
        _ => origin,
    }
}

const fn want(input: Input) -> u32 {
    if matches!(input.screen, Screen::Off) {
        RELAX
    } else if input.engaged {
        EXPEDITE
    } else {
        NORMAL
    }
}

const fn pace(regime: u32) -> u64 {
    match regime {
        EXPEDITE => ACTIVE_US,
        RELAX => DARK_US,
        _ => LIT_US,
    }
}

impl Policy {
    pub const fn new() -> Self {
        Self {
            dwell: Dwell::new(NORMAL, 0, GAP_US),
        }
    }

    pub fn step(&mut self, input: Input, now_us: u64) -> Decision {
        let wanted = want(input);

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

    fn lit(engaged: bool) -> Input {
        Input {
            screen: Screen::On,
            engaged,
        }
    }

    fn dark(engaged: bool) -> Input {
        Input {
            screen: Screen::Off,
            engaged,
        }
    }

    #[test]
    fn interaction_expedites_grace_periods() {
        assert_eq!(want(lit(true)), EXPEDITE, "touched");
        assert_eq!(want(lit(false)), NORMAL, "idle");
    }

    #[test]
    fn a_dark_screen_never_expedites() {
        assert_eq!(want(dark(false)), RELAX, "idle");
        assert_eq!(want(dark(true)), RELAX, "even a wake gesture");
    }

    #[test]
    fn the_target_is_the_device_default_unless_told_otherwise() {
        for origin in [0, 1] {
            assert_eq!(target(origin, NORMAL), origin, "normal keeps {origin}");
        }
        assert_eq!(target(0, EXPEDITE), 1, "expedite");
        assert_eq!(target(1, RELAX), 0, "relax");
        assert_eq!(target(0, 9), 0, "an unknown regime keeps the default");
    }

    #[test]
    fn an_interaction_is_answered_at_once() {
        let mut policy = Policy::new();
        assert_eq!(
            policy.step(lit(true), 10 * S).regime,
            Some(EXPEDITE),
            "first touch"
        );
    }

    #[test]
    fn regime_changes_are_spaced() {
        let mut policy = Policy::new();
        assert_eq!(policy.step(lit(true), 10 * S).regime, Some(EXPEDITE), "on");
        assert_eq!(
            policy
                .step(lit(false), 10 * S + GAP_US.saturating_sub(1))
                .regime,
            None,
            "too soon"
        );
        assert_eq!(
            policy.step(lit(false), 10 * S + GAP_US).regime,
            Some(NORMAL),
            "spaced"
        );
    }

    #[test]
    fn the_pace_follows_the_regime() {
        let mut policy = Policy::new();
        assert_eq!(policy.step(lit(false), 0).next_us, LIT_US, "idle");
        assert_eq!(policy.step(lit(true), S).next_us, ACTIVE_US, "touched");
        assert_eq!(policy.step(dark(false), 2 * S).next_us, DARK_US, "dark");
    }
}
