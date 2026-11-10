// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use crate::automata::dwell::Dwell;
use crate::automata::fixed::{at, stretch};
use crate::vm::policy::Verdict;

const LIGHT: u32 = 1;
const TOP: u32 = 3;
const VFS_SHARE: [u32; 4] = [700, 1000, 1250, 1500];
const SWAP_SHARE: [u32; 4] = [1000, 1000, 1500, 1700];
const VFS_FLOOR: u32 = 10;
const VFS_CAP: u32 = 400;
const SWAP_FLOOR: u32 = 0;
const SWAP_CAP: u32 = 100;
const SETTLE_US: u64 = 3_000_000;
const GAP_US: u64 = 10_000_000;
const CALM_US: u64 = 5_000_000;
const STRAINED_US: u64 = 2_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Origin {
    pub vfs: u32,
    pub swappiness: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    pub vfs: u32,
    pub swappiness: u32,
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
    origin.vfs > 0 || origin.swappiness > 0
}

pub fn plan(origin: Origin, regime: u32, swap: Verdict) -> Plan {
    let rung = usize::try_from(regime).unwrap_or(0);

    let swappiness = if swap == Verdict::Apply {
        stretch(
            origin.swappiness,
            at(&SWAP_SHARE, rung),
            SWAP_FLOOR,
            SWAP_CAP,
        )
    } else {
        origin.swappiness
    };

    Plan {
        vfs: stretch(origin.vfs, at(&VFS_SHARE, rung), VFS_FLOOR, VFS_CAP),
        swappiness,
    }
}

impl Policy {
    pub const fn new() -> Self {
        Self {
            dwell: Dwell::new(LIGHT, SETTLE_US, GAP_US),
        }
    }

    pub fn step(&mut self, strain: u32, now_us: u64) -> Decision {
        Decision {
            regime: self.dwell.admit(strain.min(TOP), now_us),
            next_us: if strain == 0 { CALM_US } else { STRAINED_US },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: u64 = 1_000_000;
    const DEVICE: Origin = Origin {
        vfs: 100,
        swappiness: 60,
    };

    #[test]
    fn a_light_strain_is_the_device_baseline() {
        assert_eq!(
            plan(DEVICE, LIGHT, Verdict::Apply),
            Plan {
                vfs: 100,
                swappiness: 60
            },
            "device defaults are the baseline"
        );
    }

    #[test]
    fn calm_memory_retains_the_metadata_caches() {
        let plan = plan(DEVICE, 0, Verdict::Apply);
        assert_eq!(plan.vfs, 70, "dentries and inodes are kept longer");
        assert_eq!(plan.swappiness, 60, "swapping is not discouraged");
    }

    #[test]
    fn strain_reclaims_the_caches_and_swaps_more() {
        let tight = plan(DEVICE, 2, Verdict::Apply);
        let critical = plan(DEVICE, 3, Verdict::Apply);
        assert_eq!((tight.vfs, tight.swappiness), (125, 90), "tight");
        assert_eq!((critical.vfs, critical.swappiness), (150, 100), "critical");
    }

    #[test]
    fn swappiness_is_left_alone_without_a_zram_only_swap() {
        for swap in [Verdict::Release, Verdict::Hold] {
            for regime in 0..=TOP {
                assert_eq!(
                    plan(DEVICE, regime, swap).swappiness,
                    60,
                    "{swap:?} at {regime}"
                );
            }
        }
    }

    #[test]
    fn swappiness_stays_within_the_portable_range() {
        let eager = Origin {
            vfs: 100,
            swappiness: 80,
        };
        assert_eq!(plan(eager, 3, Verdict::Apply).swappiness, 100, "capped");
        let high = Origin {
            vfs: 100,
            swappiness: 160,
        };
        assert_eq!(plan(high, 3, Verdict::Apply).swappiness, 160, "kept");
    }

    #[test]
    fn a_vendor_choice_of_no_swapping_is_respected() {
        let never = Origin {
            vfs: 100,
            swappiness: 0,
        };
        for regime in 0..=TOP {
            assert_eq!(plan(never, regime, Verdict::Apply).swappiness, 0);
        }
    }

    #[test]
    fn a_vendor_choice_of_never_reclaiming_metadata_is_respected() {
        let keep = Origin {
            vfs: 0,
            swappiness: 60,
        };
        for regime in 0..=TOP {
            assert_eq!(plan(keep, regime, Verdict::Apply).vfs, 0);
        }
    }

    #[test]
    fn metadata_reclaim_is_bounded() {
        let heavy = Origin {
            vfs: 300,
            swappiness: 60,
        };
        assert_eq!(plan(heavy, 3, Verdict::Hold).vfs, 400, "capped");
        let light = Origin {
            vfs: 12,
            swappiness: 60,
        };
        assert_eq!(plan(light, 0, Verdict::Hold).vfs, 10, "floored");
    }

    #[test]
    fn nothing_is_tunable_without_either_knob() {
        let none = Origin {
            vfs: 0,
            swappiness: 0,
        };
        assert!(!tunable(none), "both disabled");
        assert!(tunable(DEVICE), "both present");
        assert!(
            tunable(Origin {
                vfs: 0,
                swappiness: 60
            }),
            "swappiness only"
        );
    }

    #[test]
    fn a_regime_is_admitted_only_after_it_settles() {
        let mut policy = Policy::new();
        assert_eq!(policy.step(2, 4 * S).regime, None, "just started");
        assert_eq!(policy.step(2, 6 * S).regime, None, "still settling");
        assert_eq!(policy.step(2, 7 * S).regime, Some(2), "settled");
    }

    #[test]
    fn regime_changes_are_spaced() {
        let mut policy = Policy::new();
        assert_eq!(policy.step(2, 10 * S).regime, None, "settling");
        assert_eq!(policy.step(2, 13 * S).regime, Some(2), "first change");
        assert_eq!(policy.step(1, 14 * S).regime, None, "settling back");
        assert_eq!(policy.step(1, 17 * S).regime, None, "gap not over");
        assert_eq!(policy.step(1, 24 * S).regime, Some(1), "back");
    }

    #[test]
    fn strain_is_polled_faster_than_calm() {
        let mut policy = Policy::new();
        assert_eq!(policy.step(0, 0).next_us, CALM_US, "calm");
        assert_eq!(policy.step(1, S).next_us, STRAINED_US, "light strain");
    }
}
