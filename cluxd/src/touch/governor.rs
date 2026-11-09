// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use super::policy::{EVENT_LIMIT, HOLD_US, touchscreen};
use crate::abi::{self, Epoll, Fault, Fd, Outcome, READ};
use crate::event::slot::{Cause, Unit};
use crate::psi::Bus;
use crate::sysfs::path::Path;

const DEVICES: usize = 4;

type Devices = [Option<Fd>; DEVICES];

#[derive(Debug)]
pub struct Governor {
    mux: Epoll,
    devices: Devices,
}

fn device(index: u32) -> Option<Fd> {
    let path = Path::<32>::new()
        .push(b"/dev/input/event")?
        .push_uint(u64::from(index))?;

    let fd = abi::input_open(path.cstr()?).ok()?;
    let caps = abi::input_caps(&fd).ok()?;

    touchscreen(caps).then_some(fd)
}

impl Unit for Governor {
    const STARTED: &'static CStr = c"touch monitor online";
    const FAILED: &'static CStr = c"touch monitor stopped, fault";
    const WATCH: u32 = READ;

    fn spawn(_now_us: u64) -> Outcome<Self> {
        let mux = Epoll::open()?;
        let mut devices = Devices::default();

        let found = (0..EVENT_LIMIT).filter_map(device);
        for (token, (slot, fd)) in (0_u64..).zip(devices.iter_mut().zip(found)) {
            mux.add(&fd, token, READ)?;
            *slot = Some(fd);
        }

        if devices.iter().all(Option::is_none) {
            return Err(Fault::Absent);
        }

        Ok(Self { mux, devices })
    }

    fn watch(&self) -> Option<&Fd> {
        Some(self.mux.fd())
    }

    fn tick(&mut self, now_us: u64, bus: &mut Bus, _cause: Cause) -> Outcome<u64> {
        let mut seen = 0_u32;

        for fd in self.devices.iter().flatten() {
            seen = seen.saturating_add(abi::input_drain(fd)?);
        }

        if seen > 0 {
            bus.touch_until = now_us.saturating_add(HOLD_US);
        }

        Ok(u64::MAX)
    }

    fn retire(&mut self) {}

    fn vacate(bus: &mut Bus) {
        bus.touch_until = 0;
    }
}
