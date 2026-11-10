// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use super::policy::{Action, Input, Policy, Stat, parse};
use crate::abi::{self, Fault, Fd, Mode, Outcome};
use crate::event::slot::{Cause, Unit};
use crate::psi::Bus;
use crate::sysfs::knob::Knob;
use crate::vm::policy::{Verdict, judge};

const COMPACT: &CStr = c"/sys/block/zram0/compact";
const MM_STAT: &CStr = c"/sys/block/zram0/mm_stat";
const SWAPS: &CStr = c"/proc/swaps";
const SWAPS_CAP: usize = 512;
const STAT_CAP: usize = 160;
const SWAPS_PERIOD_US: u64 = 60_000_000;
const TRIGGER: u32 = 1;

#[derive(Debug)]
pub struct Governor {
    compact: Knob,
    stat: Fd,
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

    fn read(&self) -> Outcome<Option<Stat>> {
        let mut buf = [0_u8; STAT_CAP];
        let got = self.stat.read(&mut buf)?;

        Ok(buf.get(..got).and_then(parse))
    }

    fn compaction(&mut self, before: Option<Stat>, now_us: u64) -> Outcome<()> {
        let started = self.compact.force(TRIGGER);
        let after = self.read()?;

        let gained = match (started, before, after) {
            (true, Some(old), Some(new)) => new.compacted.saturating_sub(old.compacted),
            _ => 0,
        };

        self.policy.done(gained, now_us);
        Ok(())
    }
}

impl Unit for Governor {
    const STARTED: &'static CStr = c"zram governor online";
    const FAILED: &'static CStr = c"zram governor stopped, fault";

    fn spawn(_now_us: u64) -> Outcome<Self> {
        let stat = abi::open(MM_STAT, Mode::Read).map_err(|_| Fault::Absent)?;
        let compact = Knob::open(COMPACT);

        if !compact.present() {
            return Err(Fault::Absent);
        }

        Ok(Self {
            compact,
            stat,
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

        let before = self.read()?;
        let action = self.policy.step(
            &Input {
                mem: bus.mem,
                cpu: bus.cpu,
                io: bus.io,
                screen: bus.screen,
                plugged: bus.plugged,
                engaged: bus.touch_until > now_us,
                swap: self.swap,
                stat: before,
            },
            now_us,
        );

        match action {
            Action::Wait(next_us) => Ok(now_us.saturating_add(next_us)),
            Action::Compact => {
                self.compaction(before, now_us)?;
                Ok(now_us.saturating_add(Policy::watch_us()))
            }
        }
    }

    fn retire(&mut self) {}

    fn vacate(_bus: &mut Bus) {}
}
