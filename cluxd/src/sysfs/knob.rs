// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use super::lexer;
use super::path::Decimal;
use crate::abi::{self, Fd, Mode};
use crate::log;

const STRIKES: u8 = 3;
const READ_CAP: usize = 32;

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
}

impl Knob {
    pub fn open(path: &CStr) -> Self {
        let node = abi::open(path, Mode::ReadWrite)
            .or_else(|_| abi::open(path, Mode::Write))
            .ok();

        let origin = node.as_ref().and_then(Self::probe);
        Self {
            node,
            origin,
            last: None,
            breaker: Breaker::default(),
            muted: false,
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

        let Some(node) = self.node.as_ref() else {
            return;
        };

        if node.write(Decimal::new(u64::from(value)).line()).is_ok() {
            self.last = Some(value);
            self.breaker.pass();
            return;
        }

        if self.breaker.trip() {
            self.muted = true;
            log::warn(c"knob disabled after repeated write failures");
        }
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
