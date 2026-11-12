// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use super::policy::{Input, Origin, Policy, plan};
use crate::abi::{Fault, Fd, Outcome};
use crate::event::slot::{Cause, Unit};
use crate::psi::Bus;
use crate::sysfs::knob::Knob;

const TOP_MIN: &CStr = c"/dev/cpuctl/top-app/cpu.uclamp.min";
const TOP_LATENCY: &CStr = c"/dev/cpuctl/top-app/cpu.uclamp.latency_sensitive";
const BACKGROUND_MAX: &CStr = c"/dev/cpuctl/background/cpu.uclamp.max";
const BOOST: &CStr = c"/dev/stune/top-app/schedtune.boost";
const PREFER_IDLE: &CStr = c"/dev/stune/top-app/schedtune.prefer_idle";

#[derive(Debug)]
pub struct Governor {
    min: Knob,
    latency: Knob,
    max: Knob,
    boost: Knob,
    prefer: Knob,
    origin: Origin,
    policy: Policy,
}

fn drive(knob: &mut Knob, value: u32) {
    if knob.origin().is_some() {
        knob.set(value);
    }
}

impl Governor {
    fn apply(&mut self, regime: u32) {
        let plan = plan(self.origin, regime);

        drive(&mut self.min, plan.min);
        drive(&mut self.latency, plan.latency);
        drive(&mut self.max, plan.max);
        drive(&mut self.boost, plan.boost);
        drive(&mut self.prefer, plan.prefer);
    }
}

impl Unit for Governor {
    const STARTED: &'static CStr = c"cgroup governor online";
    const FAILED: &'static CStr = c"cgroup governor stopped, fault";

    fn spawn(_now_us: u64) -> Outcome<Self> {
        let min = Knob::percent(TOP_MIN);
        let latency = Knob::open(TOP_LATENCY);
        let max = Knob::percent(BACKGROUND_MAX);
        let boost = Knob::open(BOOST);
        let prefer = Knob::open(PREFER_IDLE);

        let origin = Origin {
            min: min.origin().unwrap_or(0),
            latency: latency.origin().unwrap_or(0),
            max: max.origin().unwrap_or(0),
            boost: boost.origin().unwrap_or(0),
            prefer: prefer.origin().unwrap_or(0),
        };

        let usable = [&min, &latency, &max, &boost, &prefer]
            .iter()
            .any(|knob| knob.origin().is_some());

        if !usable {
            return Err(Fault::Absent);
        }

        Ok(Self {
            min,
            latency,
            max,
            boost,
            prefer,
            origin,
            policy: Policy::new(),
        })
    }

    fn watch(&self) -> Option<&Fd> {
        None
    }

    fn tick(&mut self, now_us: u64, bus: &mut Bus, _cause: Cause) -> Outcome<u64> {
        let decision = self.policy.step(
            Input {
                screen: bus.screen,
                engaged: bus.touch_until > now_us,
                plugged: bus.plugged,
            },
            now_us,
        );

        if let Some(regime) = decision.regime {
            self.apply(regime);
        }

        Ok(now_us.saturating_add(decision.next_us))
    }

    fn retire(&mut self) {
        self.min.restore();
        self.latency.restore();
        self.max.restore();
        self.boost.restore();
        self.prefer.restore();
    }

    fn vacate(_bus: &mut Bus) {}
}
