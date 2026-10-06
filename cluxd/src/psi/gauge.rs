// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use crate::abi::{self, Fault, Fd, Mode, Outcome};
use crate::automata::envelope::Envelope;
use crate::automata::fixed::ratio;
use crate::sysfs::lexer;

const READ_CAP: usize = 256;
const MIN_SPAN_US: u64 = 10_000;
const FULL_SCALE: u32 = 1000;
const ATTACK: u32 = 2;
const RELEASE: u32 = 8;
const SNAP: u32 = 250;

#[derive(Clone, Copy, Debug)]
pub struct Trigger {
    pub threshold_us: u32,
    pub window_us: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sample {
    pub level: u32,
    pub slope: i32,
}

#[derive(Debug)]
pub struct Gauge {
    watch: Option<Fd>,
    reader: Fd,
    envelope: Envelope,
    total: u64,
    stamp: u64,
    primed: bool,
}

impl Gauge {
    pub fn open(path: &CStr, trigger: Trigger) -> Outcome<Self> {
        let reader = abi::open(path, Mode::Read)?;
        let watch = abi::arm(path, trigger.threshold_us, trigger.window_us).ok();
        Ok(Self {
            watch,
            reader,
            envelope: Envelope::new(ATTACK, RELEASE, SNAP),
            total: 0,
            stamp: 0,
            primed: false,
        })
    }

    pub const fn watch(&self) -> Option<&Fd> {
        self.watch.as_ref()
    }

    pub fn sample(&mut self, now_us: u64, stale_us: u64) -> Outcome<Sample> {
        let mut buf = [0_u8; READ_CAP];
        let got = self.reader.read(&mut buf)?;
        let text = buf.get(..got).ok_or(Fault::Parse)?;

        let some = lexer::line(text, b"some ").ok_or(Fault::Parse)?;
        let total = lexer::field(some, b"total")
            .and_then(lexer::uint)
            .ok_or(Fault::Parse)?;

        let span = now_us.saturating_sub(self.stamp);

        if !self.primed {
            let seed = lexer::field(some, b"avg10")
                .and_then(lexer::centi)
                .unwrap_or(0);

            self.envelope.reset((seed / 10).min(FULL_SCALE));
        } else if span >= stale_us {
            self.envelope.reset(self.measure(total, span));
        } else if span >= MIN_SPAN_US {
            let _ = self.envelope.feed(self.measure(total, span));
        } else {
            return Ok(self.current());
        }

        self.primed = true;
        self.total = total;
        self.stamp = now_us;
        Ok(self.current())
    }

    fn measure(&self, total: u64, span: u64) -> u32 {
        ratio(
            total.saturating_sub(self.total),
            span,
            u64::from(FULL_SCALE),
        )
        .min(FULL_SCALE)
    }

    const fn current(&self) -> Sample {
        Sample {
            level: self.envelope.level(),
            slope: self.envelope.slope(),
        }
    }
}
