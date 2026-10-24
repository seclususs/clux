// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use super::policy::{Verdict, judge};
use crate::abi::{self, Fault, Fd, Outcome};
use crate::automata::cadence::Cadence;
use crate::event::slot::{Cause, Unit};
use crate::psi::Bus;
use crate::sysfs::knob::Knob;

const SWAPS: &CStr = c"/proc/swaps";
const PAGE_CLUSTER: &CStr = c"/proc/sys/vm/page-cluster";
const SWAPS_CAP: usize = 512;
const NO_READAHEAD: u32 = 0;
const PACE_MS: [u32; 3] = [30_000, 120_000, 600_000];
const PACE_PATIENCE: u32 = 2;

#[derive(Debug)]
pub struct Governor {
    knob: Knob,
    cadence: Cadence<3>,
    verdict: Verdict,
}

impl Governor {
    fn enforce(&mut self) {
        let drifted = self.knob.level().is_some_and(|level| level != NO_READAHEAD);

        if drifted {
            let _ = self.knob.force(NO_READAHEAD);
        }
    }
}

impl Unit for Governor {
    const STARTED: &'static CStr = c"vm governor online";
    const FAILED: &'static CStr = c"vm governor stopped, fault";

    fn spawn(_now_us: u64) -> Outcome<Self> {
        let knob = Knob::open(PAGE_CLUSTER);
        if knob.origin().is_none_or(|origin| origin == NO_READAHEAD) {
            return Err(Fault::Absent);
        }

        let mut cadence = Cadence::new(PACE_MS, PACE_PATIENCE);
        cadence.boost();

        Ok(Self {
            knob,
            cadence,
            verdict: Verdict::Hold,
        })
    }

    fn watch(&self) -> Option<&Fd> {
        None
    }

    fn tick(&mut self, now_us: u64, _bus: &mut Bus, _cause: Cause) -> Outcome<u64> {
        let mut buf = [0_u8; SWAPS_CAP];
        let got = abi::slurp(SWAPS, &mut buf)?;
        let text = buf.get(..got).ok_or(Fault::Parse)?;
        let verdict = judge(text, got < SWAPS_CAP);

        match verdict {
            Verdict::Apply => self.enforce(),
            Verdict::Release => self.knob.restore(),
            Verdict::Hold => {}
        }

        if verdict != self.verdict {
            self.verdict = verdict;
            self.cadence.boost();
        }

        Ok(now_us.saturating_add(self.cadence.next_us(usize::MAX)))
    }

    fn retire(&mut self) {
        self.knob.restore();
    }

    fn vacate(_bus: &mut Bus) {}
}
