// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use super::policy::{Input, Plan, Policy};
use crate::abi::{Fault, Fd, Outcome};
use crate::event::slot::{Cause, Unit};
use crate::power::budget::{Budget, Reading};
use crate::power::sensor::{self, Reader};
use crate::psi::Bus;
use crate::psi::gauge::{Gauge, Trigger};
use crate::sysfs::knob::Knob;
use crate::sysfs::probe;

const PRESSURE: &CStr = c"/proc/pressure/cpu";
const BATTERY_TEMP: &CStr = c"/sys/class/power_supply/battery/temp";
const BATTERY_LEVEL: &CStr = c"/sys/class/power_supply/battery/capacity";
const NODES: [&CStr; 6] = [
    c"/proc/sys/kernel/sched_latency_ns",
    c"/proc/sys/kernel/sched_min_granularity_ns",
    c"/proc/sys/kernel/sched_wakeup_granularity_ns",
    c"/proc/sys/kernel/sched_migration_cost_ns",
    c"/proc/sys/kernel/sched_walt_init_task_load_pct",
    c"/proc/sys/kernel/sched_uclamp_util_min",
];
const TRIGGER: Trigger = Trigger {
    threshold_us: 200_000,
    window_us: 2_000_000,
};
const BATTERY_PERIOD_US: u64 = 5_000_000;
const STALE_SLACK_US: u64 = 1_000_000;

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
        let values = [
            plan.latency,
            plan.granularity,
            plan.wakeup,
            plan.migration,
            plan.walt,
            plan.uclamp,
        ];

        for (knob, value) in self.knobs.iter_mut().zip(values) {
            knob.set(value);
        }
    }
}

impl Unit for Governor {
    const STARTED: &'static CStr = c"cpu governor online";
    const FAILED: &'static CStr = c"cpu governor stopped, fault";

    fn spawn(_now_us: u64) -> Outcome<Self> {
        let gauge = Gauge::open(PRESSURE, TRIGGER)?;

        let knobs = NODES.map(Knob::open);
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
}
