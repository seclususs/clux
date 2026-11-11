// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use super::policy::{Input, Policy, target};
use crate::abi::{self, Fault, Fd, Mode, Outcome};
use crate::event::slot::{Cause, Unit};
use crate::psi::Bus;
use crate::sysfs::knob::Knob;
use crate::sysfs::lexer;

const EXPEDITED: &CStr = c"/sys/kernel/rcu_expedited";
const NORMAL_ONLY: &CStr = c"/sys/kernel/rcu_normal";
const READ_CAP: usize = 16;

#[derive(Debug)]
pub struct Governor {
    knob: Knob,
    origin: u32,
    policy: Policy,
}

fn forced_normal() -> bool {
    let Ok(node) = abi::open(NORMAL_ONLY, Mode::Read) else {
        return false;
    };

    let mut buf = [0_u8; READ_CAP];
    node.read(&mut buf)
        .ok()
        .and_then(|got| buf.get(..got))
        .and_then(lexer::uint)
        .is_some_and(|value| value != 0)
}

impl Unit for Governor {
    const STARTED: &'static CStr = c"rcu governor online";
    const FAILED: &'static CStr = c"rcu governor stopped, fault";

    fn spawn(_now_us: u64) -> Outcome<Self> {
        let knob = Knob::open(EXPEDITED);
        let origin = knob.origin().ok_or(Fault::Absent)?;

        if forced_normal() {
            return Err(Fault::Absent);
        }

        Ok(Self {
            knob,
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
            },
            now_us,
        );

        if let Some(regime) = decision.regime {
            self.knob.set(target(self.origin, regime));
        }

        Ok(now_us.saturating_add(decision.next_us))
    }

    fn retire(&mut self) {
        self.knob.restore();
    }

    fn vacate(_bus: &mut Bus) {}
}
