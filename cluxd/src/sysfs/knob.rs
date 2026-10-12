// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use super::lexer;
use super::path::{Decimal, Path};
use crate::abi::{self, Fd, Mode};
use crate::log::{self, Level};

const STRIKES: u8 = 3;
const READ_CAP: usize = 32;
const TAG_CAP: usize = 40;
const NOTE_CAP: usize = 96;

#[derive(Clone, Copy, Debug, Default)]
struct Breaker {
    strikes: u8,
}

impl Breaker {
    const fn pass(&mut self) {
        self.strikes = 0;
    }

    const fn trip(&mut self) -> bool {
        self.strikes = self.strikes.saturating_add(1);
        self.strikes >= STRIKES
    }
}

#[derive(Debug)]
pub struct Knob {
    node: Option<Fd>,
    origin: Option<u32>,
    last: Option<u32>,
    breaker: Breaker,
    muted: bool,
    tag: Path<TAG_CAP>,
}

impl Knob {
    pub fn open(path: &CStr) -> Self {
        let node = abi::open(path, Mode::ReadWrite)
            .or_else(|_| abi::open(path, Mode::Write))
            .ok();

        let origin = node.as_ref().and_then(Self::probe);
        if let Some(value) = origin {
            let _ = abi::journal(path, value);
        }

        Self {
            node,
            origin,
            last: None,
            breaker: Breaker::default(),
            muted: false,
            tag: Self::tag_of(path),
        }
    }

    pub fn first(paths: &[&CStr]) -> Self {
        paths
            .iter()
            .copied()
            .map(Self::open)
            .find(Self::present)
            .unwrap_or_else(Self::absent)
    }

    const fn absent() -> Self {
        Self {
            node: None,
            origin: None,
            last: None,
            breaker: Breaker { strikes: 0 },
            muted: false,
            tag: Path::new(),
        }
    }

    pub const fn present(&self) -> bool {
        self.node.is_some()
    }

    pub const fn origin(&self) -> Option<u32> {
        self.origin
    }

    pub fn set(&mut self, value: u32) {
        if self.muted || self.last.or(self.origin) == Some(value) {
            return;
        }

        let _ = self.write(value);
    }

    pub fn restore(&mut self) {
        let (Some(node), Some(origin)) = (self.node.as_ref(), self.origin) else {
            return;
        };

        if self.last.is_none_or(|last| last == origin) {
            return;
        }

        if node.write(Decimal::new(u64::from(origin)).line()).is_ok() {
            self.last = Some(origin);
        }
    }

    pub fn note(&self, prefix: &[u8], value: i64) {
        let text = Path::<NOTE_CAP>::new()
            .push(prefix)
            .and_then(|head| head.push(self.tag.bytes()));

        match text.as_ref().and_then(Path::cstr) {
            Some(line) => log::num(Level::Error, line, value),
            None => log::num(Level::Error, c"knob event", value),
        }
    }

    fn write(&mut self, value: u32) -> bool {
        let Some(node) = self.node.as_ref() else {
            return false;
        };

        match node.write(Decimal::new(u64::from(value)).line()) {
            Ok(_) => {
                self.last = Some(value);
                self.breaker.pass();
                true
            }
            Err(fault) => {
                if self.breaker.trip() {
                    self.muted = true;
                    self.note(b"knob disabled: ", fault.code());
                }
                false
            }
        }
    }

    fn tag_of(path: &CStr) -> Path<TAG_CAP> {
        let name = path
            .to_bytes()
            .rsplit(|&byte| byte == b'/')
            .next()
            .unwrap_or_default();

        Path::new().push(name).unwrap_or_else(Path::new)
    }

    fn probe(node: &Fd) -> Option<u32> {
        let mut buf = [0_u8; READ_CAP];
        let got = node.read(&mut buf).ok()?;
        let value = lexer::uint(buf.get(..got)?)?;
        u32::try_from(value).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_keeps_only_the_node_name() {
        let tag = Knob::tag_of(c"/sys/dev/block/254:0/queue/nr_requests");
        assert_eq!(tag.bytes(), b"nr_requests", "basename of a sysfs path");
    }

    #[test]
    fn tag_of_a_bare_name_is_the_name() {
        let tag = Knob::tag_of(c"debug_mask");
        assert_eq!(tag.bytes(), b"debug_mask", "no slash keeps the whole name");
    }

    #[test]
    fn tag_too_long_for_the_buffer_is_dropped() {
        let tag = Knob::tag_of(c"/x/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        assert!(
            tag.bytes().is_empty(),
            "oversized names must not be truncated"
        );
    }

    #[test]
    fn breaker_trips_on_the_third_consecutive_failure() {
        let mut breaker = Breaker::default();
        assert!(!breaker.trip());
        assert!(!breaker.trip());
        assert!(breaker.trip());
    }

    #[test]
    fn breaker_success_clears_the_streak() {
        let mut breaker = Breaker::default();
        assert!(!breaker.trip());
        assert!(!breaker.trip());
        breaker.pass();
        assert!(!breaker.trip());
        assert!(!breaker.trip());
        assert!(breaker.trip());
    }

    #[test]
    fn breaker_saturates_instead_of_wrapping() {
        let mut breaker = Breaker { strikes: u8::MAX };
        assert!(breaker.trip());
        assert_eq!(breaker.strikes, u8::MAX);
    }
}
