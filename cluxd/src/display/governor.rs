// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use super::policy::{self, Source};
use crate::abi::{self, Fault, Fd, Mode, Outcome};
use crate::event::slot::{Cause, Unit};
use crate::psi::{Bus, Screen};
use crate::sysfs::lexer;
use crate::sysfs::path::Path;

const BACKLIGHTS: &CStr = c"/sys/class/backlight";
const BLANK: &CStr = c"/sys/class/graphics/fb0/blank";
const LIST_CAP: usize = 512;
const READ_CAP: usize = 16;
const PANELS: usize = 4;

type Panels = [Option<Fd>; PANELS];

#[derive(Debug)]
pub struct Governor {
    source: Source,
    panels: Panels,
}

fn level(fd: &Fd) -> Option<u64> {
    let mut buf = [0_u8; READ_CAP];
    let got = fd.read(&mut buf).ok()?;
    lexer::uint(buf.get(..got)?)
}

fn brightness(name: &[u8]) -> Option<Fd> {
    let path = Path::<96>::new()
        .push(b"/sys/class/backlight/")?
        .push(name)?
        .push(b"/brightness")?;

    abi::open(path.cstr()?, Mode::Read)
        .ok()
        .filter(|fd| level(fd).is_some())
}

fn backlights() -> Panels {
    let mut panels = Panels::default();
    let mut names = [0_u8; LIST_CAP];

    let Some(listed) = abi::scan(BACKLIGHTS, &mut names)
        .ok()
        .and_then(|used| names.get(..used))
    else {
        return panels;
    };

    let found = listed
        .split(|&byte| byte == 0)
        .filter(|name| !name.is_empty())
        .filter_map(brightness);

    for (slot, fd) in panels.iter_mut().zip(found) {
        *slot = Some(fd);
    }

    panels
}

fn blank() -> Option<Fd> {
    abi::open(BLANK, Mode::Read)
        .ok()
        .filter(|fd| level(fd).is_some())
}

impl Unit for Governor {
    const STARTED: &'static CStr = c"display monitor online";
    const FAILED: &'static CStr = c"display monitor stopped, fault";

    fn spawn(_now_us: u64) -> Outcome<Self> {
        let panels = backlights();
        if panels.iter().flatten().next().is_some() {
            return Ok(Self {
                source: Source::Backlight,
                panels,
            });
        }

        let fd = blank().ok_or(Fault::Absent)?;
        Ok(Self {
            source: Source::Blank,
            panels: [Some(fd), None, None, None],
        })
    }

    fn watch(&self) -> Option<&Fd> {
        None
    }

    fn tick(&mut self, now_us: u64, bus: &mut Bus, _cause: Cause) -> Outcome<u64> {
        let mut readable = false;
        let mut lit = false;

        for level in self.panels.iter().flatten().filter_map(level) {
            readable = true;
            lit |= policy::lit(self.source, level);
        }

        if !readable {
            return Err(Fault::Parse);
        }

        let screen = policy::screen(lit);
        bus.screen = screen;
        Ok(now_us.saturating_add(policy::next_us(screen)))
    }

    fn retire(&mut self) {}

    fn vacate(bus: &mut Bus) {
        bus.screen = Screen::On;
    }
}
