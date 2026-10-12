// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use super::policy::{Pace, Verdict, adopt, judge};
use super::table::{RULES, Rule};
use crate::abi::{self, Fault, Fd, Outcome};
use crate::event::slot::{Cause, Unit};
use crate::psi::Bus;
use crate::sysfs::knob::Knob;

const DEBUGGABLE: &CStr = c"ro.debuggable";

#[derive(Debug)]
struct Entry {
    knob: Knob,
    floor: u32,
    reached: u32,
    applied: bool,
    drifts: u8,
    surrendered: bool,
}

impl Entry {
    fn open(rule: Rule) -> Self {
        Self {
            knob: Knob::open(rule.path),
            floor: rule.floor,
            reached: rule.floor,
            applied: false,
            drifts: 0,
            surrendered: false,
        }
    }

    fn confirm(&mut self) {
        self.reached = adopt(self.reached, self.knob.level());
    }

    fn enforce(&mut self) -> bool {
        if self.surrendered {
            return false;
        }

        let Some(current) = self.knob.level() else {
            return false;
        };

        match judge(current, self.reached, self.applied, self.drifts) {
            Verdict::Hold => false,
            Verdict::Apply => {
                self.applied = self.knob.force(self.floor);
                if self.applied {
                    self.confirm();
                }
                false
            }
            Verdict::Reapply => {
                let written = self.knob.force(self.floor);
                if written {
                    self.drifts = self.drifts.saturating_add(1);
                    self.confirm();
                }
                written
            }
            Verdict::Surrender => {
                self.surrendered = true;
                self.knob
                    .note(b"debug node keeps returning: ", i64::from(current));
                true
            }
        }
    }
}

fn debuggable() -> bool {
    let mut buf = [0_u8; 8];
    abi::prop(DEBUGGABLE, &mut buf)
        .ok()
        .and_then(|len| buf.get(..len))
        .is_some_and(|value| value == b"1")
}

#[derive(Debug)]
pub struct Governor {
    rules: [Entry; RULES.len()],
    pace: Pace,
}

impl Unit for Governor {
    const STARTED: &'static CStr = c"quiet governor online";
    const FAILED: &'static CStr = c"quiet governor stopped, fault";

    fn spawn(_now_us: u64) -> Outcome<Self> {
        if debuggable() {
            return Err(Fault::Absent);
        }

        let rules = RULES.map(Entry::open);
        if rules.iter().all(|entry| !entry.knob.present()) {
            return Err(Fault::Absent);
        }

        Ok(Self {
            rules,
            pace: Pace::new(),
        })
    }

    fn watch(&self) -> Option<&Fd> {
        None
    }

    fn tick(&mut self, now_us: u64, _bus: &mut Bus, _cause: Cause) -> Outcome<u64> {
        let mut drifted = false;
        for entry in &mut self.rules {
            drifted |= entry.enforce();
        }

        Ok(now_us.saturating_add(self.pace.next_us(drifted)))
    }

    fn retire(&mut self) {
        for entry in &mut self.rules {
            entry.knob.restore();
        }
    }

    fn vacate(_bus: &mut Bus) {}
}
