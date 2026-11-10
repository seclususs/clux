// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use super::policy::{self, INTERVAL_US, Status};
use crate::abi::{self, Fault, Fd, Mode, Outcome};
use crate::event::slot::{Cause, Unit};
use crate::power::sensor::Reader;
use crate::psi::Bus;

const STATUS: &CStr = c"/sys/class/power_supply/battery/status";
const CURRENT: &CStr = c"/sys/class/power_supply/battery/current_now";
const READ_CAP: usize = 24;

#[derive(Debug)]
pub struct Governor {
    status: Fd,
    current: Reader,
    plugged: bool,
}

impl Unit for Governor {
    const STARTED: &'static CStr = c"charger monitor online";
    const FAILED: &'static CStr = c"charger monitor stopped, fault";

    fn spawn(_now_us: u64) -> Outcome<Self> {
        Ok(Self {
            status: abi::open(STATUS, Mode::Read).map_err(|_| Fault::Absent)?,
            current: Reader::open(CURRENT),
            plugged: false,
        })
    }

    fn watch(&self) -> Option<&Fd> {
        None
    }

    fn tick(&mut self, now_us: u64, bus: &mut Bus, _cause: Cause) -> Outcome<u64> {
        let mut buf = [0_u8; READ_CAP];
        let got = self.status.read(&mut buf)?;
        let status = policy::status(buf.get(..got).unwrap_or_default());

        let current = if status == Status::Unknown {
            self.current.read()
        } else {
            None
        };

        self.plugged = policy::plugged(status, current, self.plugged);
        bus.plugged = self.plugged;
        Ok(now_us.saturating_add(INTERVAL_US))
    }

    fn retire(&mut self) {}

    fn vacate(bus: &mut Bus) {
        bus.plugged = false;
    }
}
