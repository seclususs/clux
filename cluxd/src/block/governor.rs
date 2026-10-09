// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use super::policy::{Input, Plan, Policy};
use super::stat::{self, Counters, Metrics};
use crate::abi::{self, Fault, Fd, Mode, Outcome};
use crate::automata::fixed::scaled;
use crate::event::slot::{Cause, Unit};
use crate::psi::Bus;
use crate::psi::gauge::{Gauge, Trigger};
use crate::sysfs::disk::Disk;
use crate::sysfs::knob::Knob;

const PRESSURE: &CStr = c"/proc/pressure/io";
const TRIGGER: Trigger = Trigger {
    threshold_us: 500_000,
    window_us: 2_000_000,
};
const STALE_SLACK_US: u64 = 1_000_000;
const STAT_CAP: usize = 256;
const MIN_DEPTH: u32 = 4;

#[derive(Debug)]
pub struct Governor {
    gauge: Gauge,
    stat: Fd,
    last: Option<Counters>,
    read_ahead: Knob,
    depth: Knob,
    policy: Policy,
}

fn depth_for(origin: u32, share: u32) -> u32 {
    scaled(origin, share).max(MIN_DEPTH.min(origin))
}

impl Governor {
    fn counters(&self) -> Outcome<Counters> {
        let mut buf = [0_u8; STAT_CAP];
        let got = self.stat.read(&mut buf)?;
        buf.get(..got).and_then(stat::parse).ok_or(Fault::Parse)
    }

    fn apply(&mut self, plan: Plan) {
        self.read_ahead.set(plan.read_ahead);
        if let Some(origin) = self.depth.origin() {
            self.depth.set(depth_for(origin, plan.share));
        }
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

        let ahead_path = disk.lead(b"/queue/read_ahead_kb").ok_or(Fault::Parse)?;
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

    fn vacate(bus: &mut Bus) {
        bus.io = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_follows_the_device_default_not_a_fixed_table() {
        let ladder = |origin| [1000, 750, 500, 250].map(|share| depth_for(origin, share));
        assert_eq!(ladder(256), [256, 192, 128, 64]);
        assert_eq!(ladder(64), [64, 48, 32, 16]);
        assert_eq!(ladder(32), [32, 24, 16, 8]);
    }

    #[test]
    fn depth_never_exceeds_the_device_default() {
        for origin in [1_u32, 2, 4, 31, 32, 63, 64, 127, 128, 255, 256, 1024] {
            for share in [0_u32, 250, 500, 750, 1000] {
                assert!(depth_for(origin, share) <= origin.max(MIN_DEPTH));
            }
        }
    }

    #[test]
    fn depth_keeps_the_block_layer_floor() {
        assert_eq!(depth_for(8, 250), 4);
        assert_eq!(depth_for(4, 250), 4);
        assert_eq!(depth_for(2, 250), 2);
    }
}
