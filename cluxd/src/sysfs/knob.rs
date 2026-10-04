// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use super::lexer;
use super::path::Decimal;
use crate::abi::{self, Fd, Mode};
use crate::log;

const STRIKES: u8 = 3;
const READ_CAP: usize = 32;

#[derive(Debug)]
pub struct Knob {
    node: Option<Fd>,
    origin: Option<u32>,
    last: Option<u32>,
    strikes: u8,
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
            strikes: 0,
        }
    }

    pub const fn present(&self) -> bool {
        self.node.is_some()
    }

    pub fn set(&mut self, value: u32) {
        if self.last == Some(value) {
            return;
        }

        let Some(node) = self.node.as_ref() else {
            return;
        };

        if node.write(Decimal::new(u64::from(value)).line()).is_ok() {
            self.last = Some(value);
            self.strikes = 0;
            return;
        }

        self.strikes = self.strikes.saturating_add(1);
        if self.strikes >= STRIKES {
            self.node = None;
            log::warn(c"knob disabled after repeated write failures");
        }
    }

    pub fn restore(&mut self) {
        if let (Some(node), Some(origin)) = (self.node.as_ref(), self.origin) {
            let _ = node.write(Decimal::new(u64::from(origin)).line());
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
