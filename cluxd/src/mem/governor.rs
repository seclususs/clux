// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use super::policy::Policy;
use crate::abi::{Fd, Outcome};
use crate::event::slot::{Cause, Unit};
use crate::psi::Bus;
use crate::psi::gauge::{Gauge, Trigger};

const PRESSURE: &CStr = c"/proc/pressure/memory";
const TRIGGER: Trigger = Trigger {
    threshold_us: 100_000,
    window_us: 2_000_000,
};
const STALE_SLACK_US: u64 = 1_000_000;

#[derive(Debug)]
pub struct Governor {
    gauge: Gauge,
    policy: Policy,
}

impl Unit for Governor {
    const STARTED: &'static CStr = c"memory governor online";
    const FAILED: &'static CStr = c"memory governor stopped, fault";

    fn spawn(_now_us: u64) -> Outcome<Self> {
        Ok(Self {
            gauge: Gauge::open(PRESSURE, TRIGGER)?,
            policy: Policy::new(),
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
        let decision = self
            .policy
            .step(sample.level, now_us, cause == Cause::Trigger);

        bus.mem = decision.strain;
        Ok(now_us.saturating_add(decision.next_us))
    }

    fn retire(&mut self) {}

    fn vacate(bus: &mut Bus) {
        bus.mem = 0;
    }
}
