// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use crate::automata::dwell::Dwell;
use crate::psi::Screen;

const NORMAL: u32 = 0;
const INTERACTIVE: u32 = 1;
const DORMANT: u32 = 2;
const RESTING: u32 = 3;
const BOOST_CLAMP: u32 = 20;
const BOOST_STUNE: u32 = 10;
const DORMANT_CAP: u32 = 50;
const GAP_US: u64 = 500_000;
const ACTIVE_US: u64 = 500_000;
const LIT_US: u64 = 3_000_000;
const DARK_US: u64 = 5_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Origin {
    pub min: u32,
    pub latency: u32,
    pub max: u32,
    pub boost: u32,
    pub prefer: u32,
}

pub type Plan = Origin;

#[derive(Clone, Copy, Debug)]
pub struct Input {
    pub screen: Screen,
    pub engaged: bool,
    pub plugged: bool,
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

pub const fn plan(origin: Origin, regime: u32) -> Plan {
    match regime {
        INTERACTIVE => Plan {
            min: if origin.min > BOOST_CLAMP {
                origin.min
            } else {
                BOOST_CLAMP
            },
            latency: 1,
            max: origin.max,
            boost: if origin.boost > BOOST_STUNE {
                origin.boost
            } else {
                BOOST_STUNE
            },
            prefer: 1,
        },
        DORMANT => Plan {
            min: 0,
            latency: 0,
            max: if origin.max < DORMANT_CAP {
                origin.max
            } else {
                DORMANT_CAP
            },
            boost: 0,
            prefer: 0,
        },
        RESTING => Plan {
            min: 0,
            latency: 0,
            max: origin.max,
            boost: 0,
            prefer: 0,
        },
        _ => origin,
    }
}

const fn want(input: Input) -> u32 {
    if matches!(input.screen, Screen::Off) {
        if input.plugged { RESTING } else { DORMANT }
    } else if input.engaged {
        INTERACTIVE
    } else {
        NORMAL
    }
}

const fn pace(regime: u32) -> u64 {
    match regime {
        INTERACTIVE => ACTIVE_US,
        DORMANT | RESTING => DARK_US,
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
    const DEVICE: Origin = Origin {
        min: 0,
        latency: 1,
        max: 100,
        boost: 0,
        prefer: 1,
    };

    fn input(screen: Screen, engaged: bool, plugged: bool) -> Input {
        Input {
            screen,
            engaged,
            plugged,
        }
    }

    #[test]
    fn the_regime_follows_the_screen_touch_and_power() {
        assert_eq!(want(input(Screen::On, false, false)), NORMAL, "idle");
        assert_eq!(want(input(Screen::On, true, false)), INTERACTIVE, "touched");
        assert_eq!(want(input(Screen::On, true, true)), INTERACTIVE, "charging");
        assert_eq!(want(input(Screen::Off, false, false)), DORMANT, "dark");
        assert_eq!(
            want(input(Screen::Off, true, false)),
            DORMANT,
            "wake gesture"
        );
        assert_eq!(
            want(input(Screen::Off, false, true)),
            RESTING,
            "dark, plugged"
        );
    }

    #[test]
    fn a_quiet_lit_device_keeps_its_own_settings() {
        assert_eq!(plan(DEVICE, NORMAL), DEVICE, "device defaults");
        assert_eq!(plan(DEVICE, 9), DEVICE, "an unknown regime keeps them too");
    }

    #[test]
    fn interaction_favours_the_foreground_group() {
        assert_eq!(
            plan(DEVICE, INTERACTIVE),
            Origin {
                min: 20,
                latency: 1,
                max: 100,
                boost: 10,
                prefer: 1
            },
            "foreground floor, idle cpu preference"
        );
    }

    #[test]
    fn interaction_never_lowers_a_higher_device_floor() {
        let eager = Origin {
            min: 40,
            boost: 30,
            ..DEVICE
        };
        let plan = plan(eager, INTERACTIVE);
        assert_eq!(plan.min, 40, "clamp floor kept");
        assert_eq!(plan.boost, 30, "boost kept");
    }

    #[test]
    fn interaction_leaves_the_background_cap_alone() {
        let capped = Origin { max: 35, ..DEVICE };
        assert_eq!(plan(capped, INTERACTIVE).max, 35, "cap untouched");
    }

    #[test]
    fn a_dark_screen_relaxes_the_foreground_and_caps_the_background() {
        assert_eq!(
            plan(DEVICE, DORMANT),
            Origin {
                min: 0,
                latency: 0,
                max: 50,
                boost: 0,
                prefer: 0
            },
            "nobody is waiting"
        );
    }

    #[test]
    fn a_dark_screen_never_loosens_a_tighter_device_cap() {
        let tight = Origin { max: 30, ..DEVICE };
        assert_eq!(plan(tight, DORMANT).max, 30, "device cap kept");
    }

    #[test]
    fn external_power_keeps_background_work_at_full_speed() {
        let resting = plan(DEVICE, RESTING);
        assert_eq!(resting.max, 100, "no cap while plugged in");
        assert_eq!((resting.min, resting.latency), (0, 0), "foreground relaxed");
        assert_eq!(
            (resting.boost, resting.prefer),
            (0, 0),
            "foreground relaxed"
        );
    }

    #[test]
    fn an_interaction_is_answered_at_once() {
        let mut policy = Policy::new();
        assert_eq!(
            policy.step(input(Screen::On, true, false), 10 * S).regime,
            Some(INTERACTIVE),
            "first touch"
        );
    }

    #[test]
    fn regime_changes_are_spaced() {
        let mut policy = Policy::new();
        let touched = input(Screen::On, true, false);
        let idle = input(Screen::On, false, false);
        assert_eq!(policy.step(touched, 10 * S).regime, Some(INTERACTIVE), "on");
        assert_eq!(
            policy.step(idle, 10 * S + GAP_US.saturating_sub(1)).regime,
            None,
            "too soon"
        );
        assert_eq!(
            policy.step(idle, 10 * S + GAP_US).regime,
            Some(NORMAL),
            "spaced"
        );
    }

    #[test]
    fn the_pace_follows_the_regime() {
        let mut policy = Policy::new();
        assert_eq!(
            policy.step(input(Screen::On, false, false), 0).next_us,
            LIT_US,
            "idle"
        );
        assert_eq!(
            policy.step(input(Screen::On, true, false), S).next_us,
            ACTIVE_US,
            "touched"
        );
        assert_eq!(
            policy.step(input(Screen::Off, false, false), 2 * S).next_us,
            DARK_US,
            "dark"
        );
    }
}
