// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use crate::abi::{self, Outcome};
use crate::event::Reactor;

const BOOT_PROP: &CStr = c"sys.boot_completed";
const PROBE_MS: i32 = 1000;
const PROBE_LIMIT: u32 = 300;
const SETTLE_US: u64 = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Proceed,
    Stop,
}

fn booted() -> bool {
    let mut buf = [0_u8; 8];
    abi::prop(BOOT_PROP, &mut buf)
        .ok()
        .and_then(|len| buf.get(..len))
        .is_some_and(|value| value == b"1")
}

pub fn millis(span_us: u64) -> i32 {
    i32::try_from(span_us.div_ceil(1000)).unwrap_or(i32::MAX)
}

pub fn wait(reactor: &mut Reactor) -> Outcome<Flow> {
    for _ in 0..PROBE_LIMIT {
        if booted() {
            break;
        }

        if reactor.wait(PROBE_MS)?.stop {
            return Ok(Flow::Stop);
        }
    }

    let end = abi::now_us().saturating_add(SETTLE_US);

    loop {
        let left = end.saturating_sub(abi::now_us());
        if left == 0 {
            return Ok(Flow::Proceed);
        }

        if reactor.wait(millis(left))?.stop {
            return Ok(Flow::Stop);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn millis_rounds_up_and_saturates() {
        assert_eq!(millis(0), 0);
        assert_eq!(millis(1), 1);
        assert_eq!(millis(1000), 1);
        assert_eq!(millis(1001), 2);
        assert_eq!(millis(u64::MAX), i32::MAX);
    }
}
