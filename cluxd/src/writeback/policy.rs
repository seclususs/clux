// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use crate::automata::dwell::Dwell;
use crate::automata::fixed::{at, scaled};
use crate::psi::{MEM_TIGHT, Screen};

const NORMAL: u32 = 0;
const RECLAIM: u32 = 1;
const DEFER: u32 = 2;
const BATCH: u32 = 3;
const SHARES: [[u32; 4]; 4] = [
    [1000, 1000, 1000, 1000],
    [500, 750, 500, 500],
    [1000, 1000, 2000, 2000],
    [1000, 1000, 3000, 2000],
];
const BACKGROUND_FLOOR: u32 = 1;
const RATIO_FLOOR: u32 = 5;
const INTERVAL_FLOOR: u32 = 100;
const INTERVAL_CAP: u32 = 3000;
const EXPIRE_FLOOR: u32 = 100;
const EXPIRE_CAP: u32 = 6000;
const IO_BUSY: u32 = 220;
const SETTLE_US: u64 = 2_000_000;
const GAP_US: u64 = 6_000_000;
const FAST_US: u64 = 1_000_000;
const SLOW_US: u64 = 4_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Origin {
    pub background: u32,
    pub ratio: u32,
    pub interval: u32,
    pub expire: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    pub background: u32,
    pub ratio: u32,
    pub interval: u32,
    pub expire: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct Input {
    pub mem: u32,
    pub io: u32,
    pub screen: Screen,
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

pub const fn tunable(origin: Origin) -> bool {
    origin.background > 0 && origin.ratio > 0
}

const fn want(input: Input) -> u32 {
    if input.mem >= MEM_TIGHT {
        RECLAIM
    } else if input.io >= IO_BUSY {
        DEFER
    } else if matches!(input.screen, Screen::Off) && !input.plugged {
        BATCH
    } else {
        NORMAL
    }
}

fn stretch(origin: u32, share: u32, floor: u32, cap: u32) -> u32 {
    if origin == 0 {
        return 0;
    }

    scaled(origin, share).clamp(floor.min(origin), cap.max(origin))
}

pub fn plan(origin: Origin, regime: u32) -> Plan {
    let row = at(&SHARES, usize::try_from(regime).unwrap_or(0));
    let share = |column: usize| at(&row, column);

    let ratio = stretch(origin.ratio, share(1), RATIO_FLOOR, origin.ratio);
    let background = stretch(
        origin.background,
        share(0),
        BACKGROUND_FLOOR,
        origin.background,
    );

    Plan {
        background: background.min(ratio),
        ratio,
        interval: stretch(origin.interval, share(2), INTERVAL_FLOOR, INTERVAL_CAP),
        expire: stretch(origin.expire, share(3), EXPIRE_FLOOR, EXPIRE_CAP),
    }
}

impl Policy {
    pub const fn new() -> Self {
        Self {
            dwell: Dwell::new(NORMAL, SETTLE_US, GAP_US),
        }
    }

    pub fn step(&mut self, input: Input, now_us: u64) -> Decision {
        let wanted = want(input);
        let calm = wanted == NORMAL && input.mem == 0;

        Decision {
            regime: self.dwell.admit(wanted, now_us),
            next_us: if calm { SLOW_US } else { FAST_US },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: u64 = 1_000_000;
    const DEVICE: Origin = Origin {
        background: 5,
        ratio: 20,
        interval: 500,
        expire: 3000,
    };

    fn input(mem: u32, io: u32, screen: Screen, plugged: bool) -> Input {
        Input {
            mem,
            io,
            screen,
            plugged,
        }
    }

    fn lit() -> Input {
        input(0, 0, Screen::On, false)
    }

    #[test]
    fn a_quiet_lit_device_keeps_its_own_settings() {
        assert_eq!(want(lit()), NORMAL, "nothing to react to");
        assert_eq!(
            plan(DEVICE, NORMAL),
            Plan {
                background: 5,
                ratio: 20,
                interval: 500,
                expire: 3000
            },
            "device defaults are the baseline"
        );
    }

    #[test]
    fn memory_strain_outranks_everything_else() {
        let strained = input(MEM_TIGHT, 900, Screen::Off, false);
        assert_eq!(want(strained), RECLAIM, "reclaim first");
    }

    #[test]
    fn busy_storage_defers_the_background_flush() {
        assert_eq!(want(input(0, IO_BUSY, Screen::On, true)), DEFER, "io busy");
        assert_eq!(
            want(input(0, IO_BUSY.saturating_sub(1), Screen::On, true)),
            NORMAL,
            "just below the gate"
        );
    }

    #[test]
    fn a_dark_screen_on_battery_batches_the_flushes() {
        assert_eq!(want(input(0, 0, Screen::Off, false)), BATCH, "on battery");
        assert_eq!(
            want(input(0, 0, Screen::Off, true)),
            NORMAL,
            "external power does not need batching"
        );
    }

    #[test]
    fn reclaim_flushes_sooner_and_smaller() {
        assert_eq!(
            plan(DEVICE, RECLAIM),
            Plan {
                background: 2,
                ratio: 15,
                interval: 250,
                expire: 1500
            },
            "dirty memory is returned quickly"
        );
    }

    #[test]
    fn defer_and_batch_only_stretch_the_timers() {
        let defer = plan(DEVICE, DEFER);
        let batch = plan(DEVICE, BATCH);
        assert_eq!((defer.background, defer.ratio), (5, 20), "defer ratios");
        assert_eq!((batch.background, batch.ratio), (5, 20), "batch ratios");
        assert_eq!((defer.interval, defer.expire), (1000, 6000), "defer timers");
        assert_eq!((batch.interval, batch.expire), (1500, 6000), "batch timers");
    }

    #[test]
    fn ratios_are_never_raised_above_the_device_default() {
        for regime in 0..4 {
            let plan = plan(DEVICE, regime);
            assert!(plan.background <= DEVICE.background, "background {regime}");
            assert!(plan.ratio <= DEVICE.ratio, "ratio {regime}");
            assert!(plan.background <= plan.ratio, "order {regime}");
        }
    }

    #[test]
    fn ratios_keep_a_floor() {
        let tiny = Origin {
            background: 1,
            ratio: 6,
            interval: 500,
            expire: 3000,
        };
        let plan = plan(tiny, RECLAIM);
        assert_eq!(plan.background, 1, "background floor");
        assert_eq!(plan.ratio, 5, "ratio floor");
    }

    #[test]
    fn a_floor_never_lifts_a_lower_device_default() {
        let low = Origin {
            background: 1,
            ratio: 3,
            interval: 50,
            expire: 60,
        };
        let plan = plan(low, RECLAIM);
        assert_eq!(plan.ratio, 3, "device ratio below the floor is kept");
        assert_eq!(plan.interval, 50, "device interval below the floor is kept");
        assert_eq!(plan.expire, 60, "device expiry below the floor is kept");
    }

    #[test]
    fn disabled_periodic_writeback_stays_disabled() {
        let quiet = Origin {
            interval: 0,
            ..DEVICE
        };
        for regime in 0..4 {
            assert_eq!(plan(quiet, regime).interval, 0, "regime {regime}");
        }
    }

    #[test]
    fn timers_are_capped_without_undercutting_the_device() {
        let slow = Origin {
            interval: 9000,
            expire: 30_000,
            ..DEVICE
        };
        let plan = plan(slow, BATCH);
        assert_eq!(plan.interval, 9000, "device interval above the cap");
        assert_eq!(plan.expire, 30_000, "device expiry above the cap");
    }

    #[test]
    fn byte_based_limits_are_not_tunable() {
        let by_bytes = Origin {
            background: 0,
            ratio: 0,
            ..DEVICE
        };
        assert!(!tunable(by_bytes), "ratios read as zero in bytes mode");
        assert!(tunable(DEVICE), "ratio mode");
    }

    #[test]
    fn a_regime_is_admitted_only_after_it_settles() {
        let mut policy = Policy::new();
        let strained = input(MEM_TIGHT, 0, Screen::On, false);
        assert_eq!(policy.step(strained, 3 * S).regime, None, "just started");
        assert_eq!(policy.step(strained, 4 * S).regime, None, "still settling");
        assert_eq!(
            policy.step(strained, 5 * S).regime,
            Some(RECLAIM),
            "settled"
        );
    }

    #[test]
    fn a_flapping_signal_never_settles() {
        let mut policy = Policy::new();
        let strained = input(MEM_TIGHT, 0, Screen::On, false);
        for tick in 0..10 {
            let wanted = if tick % 2 == 0 { strained } else { lit() };
            let admitted = policy.step(wanted, 10 * S + tick * S).regime;
            assert_ne!(admitted, Some(RECLAIM), "flap at tick {tick}");
        }
    }

    #[test]
    fn regime_changes_are_spaced() {
        let mut policy = Policy::new();
        let strained = input(MEM_TIGHT, 0, Screen::On, false);
        assert_eq!(policy.step(strained, 10 * S).regime, None, "settling");
        assert_eq!(policy.step(strained, 12 * S).regime, Some(RECLAIM), "first");
        assert_eq!(policy.step(lit(), 13 * S).regime, None, "settling back");
        assert_eq!(policy.step(lit(), 15 * S).regime, None, "gap not over");
        assert_eq!(policy.step(lit(), 19 * S).regime, Some(NORMAL), "back");
    }

    #[test]
    fn strain_and_busy_storage_are_polled_faster() {
        let mut policy = Policy::new();
        assert_eq!(policy.step(lit(), 0).next_us, SLOW_US, "calm");
        let busy = input(0, IO_BUSY, Screen::On, false);
        assert_eq!(policy.step(busy, S).next_us, FAST_US, "busy");
        let light = input(1, 0, Screen::On, false);
        assert_eq!(policy.step(light, 2 * S).next_us, FAST_US, "light strain");
    }
}
