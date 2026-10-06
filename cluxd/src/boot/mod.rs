// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

mod gate;
mod guard;
mod serve;

use gate::Flow;

use crate::abi;
use crate::event::Reactor;
use crate::log::{self, Level};

const EXIT_OK: i32 = 0;
const EXIT_ABI: i32 = 70;
const EXIT_ROOT: i32 = 77;
const EXIT_DETACH: i32 = 71;
const EXIT_LOCK: i32 = 75;
const EXIT_FATAL: i32 = 1;

pub fn run() -> i32 {
    if !abi::compatible() {
        log::error(c"libclux ABI mismatch");
        return EXIT_ABI;
    }

    if abi::root().is_err() {
        log::error(c"root privileges are required");
        return EXIT_ROOT;
    }

    if abi::detach().is_err() {
        log::error(c"detach failed");
        return EXIT_DETACH;
    }

    let _lock = match guard::claim() {
        Ok(fd) => fd,
        Err(fault) if fault.contended() => {
            log::info(c"another instance is running");
            return EXIT_OK;
        }
        Err(fault) => {
            log::num(Level::Error, c"instance lock failed", fault.code());
            return EXIT_LOCK;
        }
    };

    guard::harden();

    let outcome = Reactor::open().and_then(|mut reactor| match gate::wait(&mut reactor)? {
        Flow::Stop => Ok(()),
        Flow::Proceed => serve::run(&mut reactor),
    });

    match outcome {
        Ok(()) => {
            log::info(c"cluxd stopped");
            EXIT_OK
        }
        Err(fault) => {
            log::num(Level::Error, c"cluxd aborted, fault", fault.code());
            EXIT_FATAL
        }
    }
}
