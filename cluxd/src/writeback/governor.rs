// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use super::policy::{Input, Origin, Policy, plan, tunable};
use crate::abi::{Fault, Fd, Outcome};
use crate::event::slot::{Cause, Unit};
use crate::psi::Bus;
use crate::sysfs::knob::Knob;

const BACKGROUND: &CStr = c"/proc/sys/vm/dirty_background_ratio";
const RATIO: &CStr = c"/proc/sys/vm/dirty_ratio";
const INTERVAL: &CStr = c"/proc/sys/vm/dirty_writeback_centisecs";
const EXPIRE: &CStr = c"/proc/sys/vm/dirty_expire_centisecs";

#[derive(Debug)]
pub struct Governor {
    knobs: [Knob; 4],
    origin: Origin,
    policy: Policy,
}

fn origin_of(knobs: &[Knob; 4]) -> Option<Origin> {
    let [background, ratio, interval, expire] = knobs;

    Some(Origin {
        background: background.origin()?,
        ratio: ratio.origin()?,
        interval: interval.origin()?,
        expire: expire.origin()?,
    })
}

impl Governor {
    fn apply(&mut self, regime: u32) {
        let plan = plan(self.origin, regime);
        let values = [plan.background, plan.ratio, plan.interval, plan.expire];

        for (knob, value) in self.knobs.iter_mut().zip(values) {
            knob.set(value);
        }
    }
}

impl Unit for Governor {
    const STARTED: &'static CStr = c"writeback governor online";
    const FAILED: &'static CStr = c"writeback governor stopped, fault";

    fn spawn(_now_us: u64) -> Outcome<Self> {
        let knobs = [
            Knob::open(BACKGROUND),
            Knob::open(RATIO),
            Knob::open(INTERVAL),
            Knob::open(EXPIRE),
        ];

        let origin = origin_of(&knobs)
            .filter(|&origin| tunable(origin))
            .ok_or(Fault::Absent)?;

        Ok(Self {
            knobs,
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
                mem: bus.mem,
                io: bus.io,
                screen: bus.screen,
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
        self.knobs.iter_mut().for_each(Knob::restore);
    }

    fn vacate(_bus: &mut Bus) {}
}
