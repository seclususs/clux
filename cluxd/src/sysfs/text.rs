// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use super::path::Path;
use crate::abi::{self, Fd, Mode};

const TEXT_CAP: usize = 32;

#[derive(Debug)]
pub struct Text {
    node: Fd,
    origin: Path<TEXT_CAP>,
    changed: bool,
}

impl Text {
    pub fn open(path: &CStr) -> Option<Self> {
        let node = abi::open(path, Mode::ReadWrite).ok()?;
        let origin = Self::read(&node)?;

        Some(Self {
            node,
            origin,
            changed: false,
        })
    }

    pub fn origin(&self) -> &[u8] {
        self.origin.bytes()
    }

    pub fn set(&mut self, value: &[u8]) -> bool {
        let Some(line) = Path::<TEXT_CAP>::new()
            .push(value)
            .and_then(|text| text.push(b"\n"))
        else {
            return false;
        };

        let written = self.node.write(line.bytes()).is_ok();
        self.changed |= written;
        written
    }

    pub fn restore(&mut self) {
        if !self.changed {
            return;
        }

        let Some(line) = Path::<TEXT_CAP>::new()
            .push(self.origin.bytes())
            .and_then(|text| text.push(b"\n"))
        else {
            return;
        };

        if self.node.write(line.bytes()).is_ok() {
            self.changed = false;
        }
    }

    fn read(node: &Fd) -> Option<Path<TEXT_CAP>> {
        let mut buf = [0_u8; TEXT_CAP];
        let got = node.read(&mut buf).ok()?;
        let text = buf.get(..got)?.trim_ascii();

        if text.is_empty() {
            return None;
        }

        Path::new().push(text)
    }
}
