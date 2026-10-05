// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use super::policy::{Input, Plan, Policy};
use super::stat::{self, Counters, Metrics};
use crate::abi::{self, Fault, Fd, Mode, Outcome};
use crate::event::slot::{Cause, Unit};
use crate::psi::Bus;
use crate::psi::gauge::{Gauge, Trigger};
use crate::sysfs::knob::Knob;
use crate::sysfs::probe::Disk;

const PRESSURE: &CStr = c"/proc/pressure/io";
const TRIGGER: Trigger = Trigger {
    threshold_us: 500_000,
    window_us: 2_000_000,
};
const STALE_SLACK_US: u64 = 1_000_000;
const STAT_CAP: usize = 256;

#[derive(Debug)]
pub struct Governor {
    gauge: Gauge,
    stat: Fd,
    last: Option<Counters>,
    read_ahead: Knob,
    depth: Knob,
    policy: Policy,
}

impl Governor {
    fn counters(&self) -> Outcome<Counters> {
        let mut buf = [0_u8; STAT_CAP];
        let got = self.stat.read(&mut buf)?;
        buf.get(..got).and_then(stat::parse).ok_or(Fault::Parse)
    }

    fn apply(&mut self, plan: Plan) {
        self.read_ahead.set(plan.read_ahead);
        self.depth.set(plan.depth);
    }
}

impl Unit for Governor {
    const STARTED: &'static CStr = c"block governor online";
    const FAILED: &'static CStr = c"block governor stopped, fault";

    fn spawn(_now_us: u64) -> Outcome<Self> {
        let disk = Disk::find().ok_or(Fault::Absent)?;
        let node = |tail: &[u8]| disk.node(tail).ok_or(Fault::Parse);
        let gauge = Gauge::open(PRESSURE, TRIGGER)?;

        let stat_path = node(b"/stat")?;
        let stat = abi::open(stat_path.cstr().ok_or(Fault::Parse)?, Mode::Read)?;

        let ahead_path = node(b"/queue/read_ahead_kb")?;
        let depth_path = node(b"/queue/nr_requests")?;

        let read_ahead = Knob::open(ahead_path.cstr().ok_or(Fault::Parse)?);
        let depth = Knob::open(depth_path.cstr().ok_or(Fault::Parse)?);

        if !read_ahead.present() && !depth.present() {
            return Err(Fault::Absent);
        }

        Ok(Self {
            gauge,
            stat,
            last: None,
            read_ahead,
            depth,
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
        bus.io = sample.level;

        let now = self.counters()?;
        let metrics = self
            .last
            .as_ref()
            .map_or_else(Metrics::default, |prev| stat::measure(prev, &now));

        self.last = Some(now);

        let decision = self.policy.step(
            Input {
                io: sample.level,
                metrics,
            },
            now_us,
            cause == Cause::Trigger,
        );

        self.apply(decision.plan);
        Ok(now_us.saturating_add(decision.next_us))
    }

    fn retire(&mut self) {
        self.read_ahead.restore();
        self.depth.restore();
    }
}
