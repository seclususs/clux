// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use super::table::{
    DARK_CEILING, ENTER, GRANULARITY_MAX, GRANULARITY_MIN, GRANULARITY_PERCENT, GRANULARITY_STEP,
    HOLD_US, IO_FLOOR, IO_HEAVY, LATENCY, LATENCY_MIN, LATENCY_SPAN, LEAVE, MIGRATION,
    MIGRATION_FLOOR, PACE_CEILING, PACE_MS, PACE_PATIENCE, PACES, RUNGS, TOUCH_FLOOR,
    VOLATILE_SLOPE, WAKEUP, WALT,
};
use crate::automata::cadence::Cadence;
use crate::automata::fixed::{at, floor_to, ratio, scaled};
use crate::automata::ladder::Ladder;
use crate::automata::markov::Markov;
use crate::power::budget::Verdict;
use crate::psi::Screen;

#[derive(Clone, Copy, Debug)]
pub struct Input {
    pub cpu: u32,
    pub slope: i32,
    pub io: u32,
    pub screen: Screen,
    pub engaged: bool,
    pub verdict: Verdict,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    pub latency: u32,
    pub granularity: u32,
    pub wakeup: u32,
    pub migration: u32,
    pub walt: u32,
    pub boost: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct Decision {
    pub plan: Plan,
    pub next_us: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct Policy {
    ladder: Ladder<RUNGS>,
    markov: Markov<RUNGS>,
    cadence: Cadence<PACES>,
    last: usize,
}

impl Policy {
    pub const fn new() -> Self {
        Self {
            ladder: Ladder::new(ENTER, LEAVE, HOLD_US),
            markov: Markov::new(),
            cadence: Cadence::new(PACE_MS, PACE_PATIENCE),
            last: 0,
        }
    }

    pub fn interval_us(&self) -> u64 {
        self.cadence.interval_us()
    }

    pub fn step(&mut self, input: Input, now_us: u64, urgent: bool) -> Decision {
        let moved = self.ladder.step(discount(input.cpu, input.io), now_us);
        let rung = self.ladder.rung();

        if moved {
            self.markov.observe(self.last, rung);
            self.last = rung;
        }

        let ceiling = match input.screen {
            Screen::On => input.verdict.ceiling,
            Screen::Off => input.verdict.ceiling.min(DARK_CEILING),
        };

        let ahead = if input.slope > 0 {
            self.markov
                .predict(rung)
                .filter(|&next| next > rung)
                .unwrap_or(rung)
        } else {
            rung
        };

        let floor = if input.engaged { TOUCH_FLOOR } else { 0 };
        let eager = ahead.max(floor).min(ceiling);
        if urgent {
            self.cadence.boost();
        }

        let next_us = self.cadence.next_us(at(&PACE_CEILING, eager));
        Decision {
            plan: plan(rung.max(floor).min(ceiling), eager, input),
            next_us,
        }
    }
}

const fn discount(cpu: u32, io: u32) -> u32 {
    if io >= IO_HEAVY && io > cpu.saturating_mul(2) {
        cpu / 4
    } else if io >= IO_FLOOR && io > cpu {
        cpu / 2
    } else {
        cpu
    }
}

fn plan(shown: usize, eager: usize, input: Input) -> Plan {
    let scale = input.verdict.scale;
    let floor = LATENCY_MIN.saturating_add(scaled(LATENCY_SPAN, 1000_u32.saturating_sub(scale)));

    let latency = at(&LATENCY, shown).max(floor);
    let granularity = floor_to(
        ratio(u64::from(latency), 100, GRANULARITY_PERCENT).clamp(GRANULARITY_MIN, GRANULARITY_MAX),
        GRANULARITY_STEP,
    );

    let base = at(&MIGRATION, shown);
    let migration = if input.slope.unsigned_abs() >= VOLATILE_SLOPE {
        (base / 2).max(MIGRATION_FLOOR)
    } else {
        base
    };

    Plan {
        latency,
        granularity,
        wakeup: at(&WAKEUP, eager),
        migration,
        walt: at(&WALT, shown),
        boost: scale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: u64 = 1_000_000;
    const FULL: Verdict = Verdict {
        scale: 1000,
        ceiling: 4,
    };

    fn input(cpu: u32, slope: i32, io: u32, verdict: Verdict) -> Input {
        Input {
            cpu,
            slope,
            io,
            screen: Screen::On,
            engaged: false,
            verdict,
        }
    }

    fn run(policy: &mut Policy, trace: &[(u32, i32, u32)], verdict: Verdict) -> Vec<Plan> {
        let mut now = 0_u64;
        trace
            .iter()
            .map(|&(cpu, slope, io)| {
                now = now.saturating_add(500_000);
                policy.step(input(cpu, slope, io, verdict), now, false).plan
            })
            .collect()
    }

    fn changes(plans: &[Plan]) -> usize {
        plans.windows(2).filter(|pair| pair[0] != pair[1]).count()
    }

    #[test]
    fn idle_system_gets_the_relaxed_plan() {
        let mut policy = Policy::new();
        let plan = policy.step(input(5, 0, 0, FULL), 0, false).plan;
        assert_eq!(plan.latency, 20_000_000);
        assert_eq!(plan.granularity, 6_500_000);
        assert_eq!(plan.wakeup, 6_500_000);
        assert_eq!(plan.migration, 600_000);
        assert_eq!(plan.walt, 10);
        assert_eq!(plan.boost, 1000);
    }

    #[test]
    fn surge_gets_the_aggressive_plan() {
        let mut policy = Policy::new();
        let plan = policy.step(input(400, 0, 0, FULL), 0, false).plan;
        assert_eq!(plan.latency, 8_000_000);
        assert_eq!(plan.granularity, 2_700_000);
        assert_eq!(plan.wakeup, 1_500_000);
        assert_eq!(plan.migration, 200_000);
        assert_eq!(plan.walt, 40);
        assert_eq!(plan.boost, 1000);
    }

    #[test]
    fn thermal_scale_raises_latency_floor_and_caps_boost() {
        let mut policy = Policy::new();
        let hot = Verdict {
            scale: 400,
            ceiling: 4,
        };
        let plan = policy.step(input(400, 0, 0, hot), 0, false).plan;
        assert_eq!(plan.latency, 15_200_000);
        assert_eq!(plan.boost, 400);
    }

    #[test]
    fn battery_ceiling_caps_the_regime() {
        let mut policy = Policy::new();
        let low = Verdict {
            scale: 1000,
            ceiling: 1,
        };
        let plan = policy.step(input(400, 0, 0, low), 0, false).plan;
        assert_eq!(plan.latency, 16_000_000);
        assert_eq!(plan.walt, 14);
    }

    #[test]
    fn io_bound_load_does_not_boost_the_cpu() {
        let mut policy = Policy::new();
        let plan = policy.step(input(100, 0, 400, FULL), 0, false).plan;
        assert_eq!(plan.latency, 20_000_000);
        let mut policy = Policy::new();
        let plan = policy.step(input(100, 0, 120, FULL), 0, false).plan;
        assert_eq!(plan.latency, 16_000_000);
        let mut policy = Policy::new();
        let plan = policy.step(input(150, 0, 120, FULL), 0, false).plan;
        assert_eq!(plan.latency, 12_000_000);
    }

    #[test]
    fn volatile_load_halves_migration_cost() {
        let mut policy = Policy::new();
        let plan = policy.step(input(100, 200, 0, FULL), 0, false).plan;
        assert_eq!(plan.migration, 200_000);
        let mut policy = Policy::new();
        let plan = policy.step(input(400, 200, 0, FULL), 0, false).plan;
        assert_eq!(plan.migration, 200_000);
    }

    #[test]
    fn learned_pattern_anticipates_wakeup_preemption() {
        let mut policy = Policy::new();
        let mut now = 0;
        for _ in 0..4 {
            for load in [100, 100, 400, 400] {
                now += 3 * S;
                let _ = policy.step(input(load, 0, 0, FULL), now, false);
            }
            for _ in 0..3 {
                now += 3 * S;
                let _ = policy.step(input(0, 0, 0, FULL), now, false);
            }
        }
        now += 3 * S;
        let calm = policy.step(input(100, 0, 0, FULL), now, false).plan;
        now += 3 * S;
        let rising = policy.step(input(100, 60, 0, FULL), now, false).plan;
        assert!(rising.wakeup <= calm.wakeup);
        assert_eq!(calm.latency, rising.latency);
    }

    #[test]
    fn burst_then_idle_changes_plan_few_times() {
        let mut policy = Policy::new();
        let mut trace = vec![(5, 0, 0); 4];
        trace.extend([(120, 80, 0), (260, 90, 0), (400, 80, 0)]);
        trace.extend([(390, 0, 0), (410, 0, 0), (380, 0, 0), (300, 0, 0)]);
        trace.extend([(40, -60, 0), (10, -30, 0)]);
        trace.extend(vec![(4, 0, 0); 30]);
        let plans = run(&mut policy, &trace, FULL);
        assert_eq!(plans.last().map(|plan| plan.latency), Some(20_000_000));
        assert!(
            changes(&plans) <= 8,
            "too many plan changes: {}",
            changes(&plans)
        );
    }

    #[test]
    fn jitter_around_a_threshold_does_not_flap() {
        let mut policy = Policy::new();
        let trace: Vec<(u32, i32, u32)> = (0..60)
            .map(|i| (if i % 2 == 0 { 95 } else { 72 }, 0, 0))
            .collect();
        let plans = run(&mut policy, &trace, FULL);
        assert!(changes(&plans) <= 1, "flapped {} times", changes(&plans));
    }

    #[test]
    fn urgent_trigger_speeds_up_polling() {
        let mut policy = Policy::new();
        let slow = policy.step(input(0, 0, 0, FULL), 0, false).next_us;
        let fast = policy.step(input(0, 0, 0, FULL), S, true).next_us;
        assert!(slow >= 5_000_000);
        assert!(fast < slow);
    }

    #[test]
    fn a_dark_screen_holds_the_regime_back() {
        let mut lit = Policy::new();
        let mut dark = Policy::new();
        let busy = input(400, 0, 0, FULL);
        let off = Input {
            screen: Screen::Off,
            ..busy
        };
        let bright = lit.step(busy, 0, false).plan;
        let dimmed = dark.step(off, 0, false).plan;
        assert_eq!(
            bright.latency,
            at(&LATENCY, 4),
            "lit screen reaches the top"
        );
        assert_eq!(
            dimmed.latency,
            at(&LATENCY, DARK_CEILING),
            "dark screen stays at the ceiling"
        );
    }

    #[test]
    fn a_dark_screen_cannot_lift_a_lower_ceiling() {
        let mut policy = Policy::new();
        let low = Verdict {
            scale: 1000,
            ceiling: 0,
        };
        let off = Input {
            screen: Screen::Off,
            ..input(400, 0, 0, low)
        };
        let plan = policy.step(off, 0, false).plan;
        assert_eq!(plan.latency, at(&LATENCY, 0), "battery ceiling still wins");
    }

    #[test]
    fn a_dark_screen_is_polled_more_slowly() {
        let mut lit = Policy::new();
        let mut dark = Policy::new();
        let busy = input(400, 0, 0, FULL);
        let off = Input {
            screen: Screen::Off,
            ..busy
        };
        assert_eq!(lit.step(busy, 0, false).next_us, 100_000, "lit and busy");
        assert_eq!(dark.step(off, 0, false).next_us, 2_500_000, "dark and busy");
    }

    #[test]
    fn touch_lifts_an_idle_system_to_the_floor() {
        let mut idle = Policy::new();
        let mut touched = Policy::new();
        let calm = input(0, 0, 0, FULL);
        let engaged = Input {
            engaged: true,
            ..calm
        };
        assert_eq!(
            idle.step(calm, 0, false).plan.latency,
            at(&LATENCY, 0),
            "idle without touch"
        );
        assert_eq!(
            touched.step(engaged, 0, false).plan.latency,
            at(&LATENCY, TOUCH_FLOOR),
            "idle with touch"
        );
    }

    #[test]
    fn touch_never_lowers_a_busy_regime() {
        let mut policy = Policy::new();
        let busy = Input {
            engaged: true,
            ..input(400, 0, 0, FULL)
        };
        let plan = policy.step(busy, 0, false).plan;
        assert_eq!(plan.latency, at(&LATENCY, 4), "load still sets the regime");
    }

    #[test]
    fn touch_cannot_lift_a_lower_ceiling() {
        let mut policy = Policy::new();
        let low = Verdict {
            scale: 1000,
            ceiling: 0,
        };
        let engaged = Input {
            engaged: true,
            ..input(0, 0, 0, low)
        };
        let plan = policy.step(engaged, 0, false).plan;
        assert_eq!(plan.latency, at(&LATENCY, 0), "budget ceiling still wins");
    }
}
