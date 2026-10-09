// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;
use core::sync::atomic::{AtomicBool, Ordering};

use super::policy::{Input, Plan, Policy};
use crate::abi::{self, Fault, Fd, Outcome};
use crate::automata::fixed::scaled;
use crate::event::slot::{Cause, Unit};
use crate::log;
use crate::power::budget::{Budget, Reading};
use crate::power::sensor::{self, Reader};
use crate::psi::Bus;
use crate::psi::gauge::{Gauge, Trigger};
use crate::sysfs::knob::Knob;
use crate::sysfs::probe;

const PRESSURE: &CStr = c"/proc/pressure/cpu";
const BATTERY_TEMP: &CStr = c"/sys/class/power_supply/battery/temp";
const BATTERY_LEVEL: &CStr = c"/sys/class/power_supply/battery/capacity";
const NODES: [&[&CStr]; 6] = [
    &[
        c"/proc/sys/kernel/sched_latency_ns",
        c"/sys/kernel/debug/sched/latency_ns",
    ],
    &[
        c"/proc/sys/kernel/sched_min_granularity_ns",
        c"/sys/kernel/debug/sched/min_granularity_ns",
        c"/sys/kernel/debug/sched/base_slice_ns",
    ],
    &[
        c"/proc/sys/kernel/sched_wakeup_granularity_ns",
        c"/sys/kernel/debug/sched/wakeup_granularity_ns",
    ],
    &[
        c"/proc/sys/kernel/sched_migration_cost_ns",
        c"/sys/kernel/debug/sched/migration_cost_ns",
    ],
    &[c"/proc/sys/kernel/sched_walt_init_task_load_pct"],
    &[c"/proc/sys/kernel/sched_util_clamp_min"],
];
const CORE_KNOBS: usize = 4;
const TRIGGER: Trigger = Trigger {
    threshold_us: 200_000,
    window_us: 2_000_000,
};
const BATTERY_PERIOD_US: u64 = 5_000_000;
const STALE_SLACK_US: u64 = 1_000_000;

static DEBUGFS_TRIED: AtomicBool = AtomicBool::new(false);

#[derive(Debug)]
pub struct Governor {
    gauge: Gauge,
    knobs: [Knob; 6],
    zone: Reader,
    battery: Reader,
    level: Reader,
    budget: Budget,
    policy: Policy,
    polled: Option<u64>,
}

fn open_knobs() -> [Knob; 6] {
    let knobs = NODES.map(Knob::first);
    let core_missing = knobs.iter().take(CORE_KNOBS).all(|knob| !knob.present());

    if core_missing && mount_debugfs() {
        return NODES.map(Knob::first);
    }

    knobs
}

fn mount_debugfs() -> bool {
    if DEBUGFS_TRIED.swap(true, Ordering::Relaxed) {
        return false;
    }

    if abi::debugfs().is_ok() {
        log::info(c"debugfs mounted privately for scheduler knobs");
        true
    } else {
        log::warn(c"debugfs unavailable, scheduler knobs skipped");
        false
    }
}

impl Governor {
    fn refresh(&mut self, now_us: u64) {
        let cpu = self.zone.read().map(sensor::from_milli);

        let due = self
            .polled
            .is_none_or(|at| now_us.saturating_sub(at) >= BATTERY_PERIOD_US);

        let (battery, level) = if due {
            self.polled = Some(now_us);
            (
                self.battery.read().map(sensor::from_deci),
                self.level
                    .read()
                    .and_then(|value| u32::try_from(value).ok()),
            )
        } else {
            (None, None)
        };

        self.budget.feed(
            Reading {
                cpu,
                battery,
                level,
            },
            now_us,
        );
    }

    fn apply(&mut self, plan: &Plan) {
        let cap = self
            .knobs
            .last()
            .and_then(|knob| boost_for(knob.origin(), plan.boost));

        let values = [
            Some(plan.latency),
            Some(plan.granularity),
            Some(plan.wakeup),
            Some(plan.migration),
            Some(plan.walt),
            cap,
        ];

        for (knob, value) in self.knobs.iter_mut().zip(values) {
            if let Some(value) = value {
                knob.set(value);
            }
        }
    }
}

fn boost_for(origin: Option<u32>, share: u32) -> Option<u32> {
    origin.map(|origin| scaled(origin, share))
}

impl Unit for Governor {
    const STARTED: &'static CStr = c"cpu governor online";
    const FAILED: &'static CStr = c"cpu governor stopped, fault";

    fn spawn(_now_us: u64) -> Outcome<Self> {
        let gauge = Gauge::open(PRESSURE, TRIGGER)?;

        let knobs = open_knobs();
        if !knobs.iter().any(Knob::present) {
            return Err(Fault::Absent);
        }

        let zone = probe::zone()
            .as_ref()
            .and_then(|path| path.cstr())
            .map_or_else(Reader::absent, Reader::open);

        Ok(Self {
            gauge,
            knobs,
            zone,
            battery: Reader::open(BATTERY_TEMP),
            level: Reader::open(BATTERY_LEVEL),
            budget: Budget::new(),
            policy: Policy::new(),
            polled: None,
        })
    }

    fn watch(&self) -> Option<&Fd> {
        self.gauge.watch()
    }

    fn tick(&mut self, now_us: u64, bus: &mut Bus, cause: Cause) -> Outcome<u64> {
        let stale = self
            .policy
            .interval_us()
            .saturating_mul(2)
            .saturating_add(STALE_SLACK_US);

        let sample = self.gauge.sample(now_us, stale)?;
        bus.cpu = sample.level;
        self.refresh(now_us);

        if self.budget.lost(now_us) {
            return Err(Fault::Errno(5));
        }

        let input = Input {
            cpu: sample.level,
            slope: sample.slope,
            io: bus.io,
            verdict: self.budget.verdict(),
        };

        let decision = self.policy.step(input, now_us, cause == Cause::Trigger);
        self.apply(&decision.plan);
        Ok(now_us.saturating_add(decision.next_us))
    }

    fn retire(&mut self) {
        self.knobs.iter_mut().for_each(Knob::restore);
    }

    fn vacate(bus: &mut Bus) {
        bus.cpu = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boost_cap_scales_the_device_default() {
        assert_eq!(boost_for(Some(1024), 1000), Some(1024));
        assert_eq!(boost_for(Some(1024), 700), Some(716));
        assert_eq!(boost_for(Some(1024), 400), Some(409));
        assert_eq!(boost_for(Some(1024), 150), Some(153));
    }

    #[test]
    fn boost_cap_never_raises_a_vendor_limit() {
        assert_eq!(boost_for(Some(512), 1000), Some(512));
        assert_eq!(boost_for(Some(512), 400), Some(204));
        assert_eq!(boost_for(Some(0), 1000), Some(0));
    }

    #[test]
    fn boost_cap_is_skipped_without_a_known_default() {
        assert_eq!(boost_for(None, 1000), None);
        assert_eq!(boost_for(None, 150), None);
    }
}
