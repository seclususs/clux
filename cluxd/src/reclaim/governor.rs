// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use super::policy::{Origin, Policy, plan, tunable};
use crate::abi::{self, Fault, Fd, Outcome};
use crate::event::slot::{Cause, Unit};
use crate::psi::Bus;
use crate::sysfs::knob::Knob;
use crate::vm::policy::{Verdict, judge};

const VFS: &CStr = c"/proc/sys/vm/vfs_cache_pressure";
const SWAPPINESS: &CStr = c"/proc/sys/vm/swappiness";
const SWAPS: &CStr = c"/proc/swaps";
const SWAPS_CAP: usize = 512;
const SWAPS_PERIOD_US: u64 = 60_000_000;

#[derive(Debug)]
pub struct Governor {
    vfs: Knob,
    swappiness: Knob,
    origin: Origin,
    policy: Policy,
    swap: Verdict,
    checked: Option<u64>,
}

fn swap_verdict() -> Outcome<Verdict> {
    let mut buf = [0_u8; SWAPS_CAP];
    let got = abi::slurp(SWAPS, &mut buf)?;
    let text = buf.get(..got).ok_or(Fault::Parse)?;

    Ok(judge(text, got < SWAPS_CAP))
}

impl Governor {
    fn refresh(&mut self, now_us: u64) -> Outcome<()> {
        let due = self
            .checked
            .is_none_or(|at| now_us.saturating_sub(at) >= SWAPS_PERIOD_US);

        if due {
            self.swap = swap_verdict()?;
            self.checked = Some(now_us);
        }

        Ok(())
    }

    fn apply(&mut self, regime: u32) {
        let plan = plan(self.origin, regime, self.swap);

        self.vfs.set(plan.vfs);
        self.swappiness.set(plan.swappiness);
    }
}

impl Unit for Governor {
    const STARTED: &'static CStr = c"reclaim governor online";
    const FAILED: &'static CStr = c"reclaim governor stopped, fault";

    fn spawn(_now_us: u64) -> Outcome<Self> {
        let vfs = Knob::open(VFS);
        let swappiness = Knob::open(SWAPPINESS);

        let origin = Origin {
            vfs: vfs.origin().unwrap_or(0),
            swappiness: swappiness.origin().unwrap_or(0),
        };

        if !tunable(origin) {
            return Err(Fault::Absent);
        }

        Ok(Self {
            vfs,
            swappiness,
            origin,
            policy: Policy::new(),
            swap: Verdict::Hold,
            checked: None,
        })
    }

    fn watch(&self) -> Option<&Fd> {
        None
    }

    fn tick(&mut self, now_us: u64, bus: &mut Bus, _cause: Cause) -> Outcome<u64> {
        self.refresh(now_us)?;

        let decision = self.policy.step(bus.mem, now_us);
        if let Some(regime) = decision.regime {
            self.apply(regime);
        }

        Ok(now_us.saturating_add(decision.next_us))
    }

    fn retire(&mut self) {
        self.vfs.restore();
        self.swappiness.restore();
    }

    fn vacate(_bus: &mut Bus) {}
}
