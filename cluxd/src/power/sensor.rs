// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use crate::abi::{self, Fd, Mode};
use crate::sysfs::lexer;

const READ_CAP: usize = 24;
const MILLI_FLOOR: u64 = 1000;

#[derive(Debug)]
pub struct Reader {
    node: Option<Fd>,
}

impl Reader {
    pub fn open(path: &CStr) -> Self {
        Self {
            node: abi::open(path, Mode::Read).ok(),
        }
    }

    pub const fn absent() -> Self {
        Self { node: None }
    }

    pub fn read(&self) -> Option<i64> {
        let mut buf = [0_u8; READ_CAP];
        let got = self.node.as_ref()?.read(&mut buf).ok()?;
        lexer::int(buf.get(..got)?)
    }
}

pub fn from_milli(raw: i64) -> i32 {
    let scaled = if raw.unsigned_abs() >= MILLI_FLOOR {
        raw / 100
    } else {
        raw.saturating_mul(10)
    };
    clip(scaled)
}

pub fn from_deci(raw: i64) -> i32 {
    clip(raw)
}

fn clip(value: i64) -> i32 {
    i32::try_from(value).unwrap_or(if value < 0 { i32::MIN } else { i32::MAX })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_units_follow_the_thermal_abi() {
        assert_eq!(from_milli(45_000), 450);
        assert_eq!(from_milli(-5_000), -50);
        assert_eq!(from_milli(5_000), 50);
        assert_eq!(from_milli(38), 380);
        assert_eq!(from_milli(0), 0);
    }

    #[test]
    fn battery_units_are_already_deci() {
        assert_eq!(from_deci(352), 352);
        assert_eq!(from_deci(i64::MAX), i32::MAX);
        assert_eq!(from_deci(i64::MIN), i32::MIN);
    }
}
