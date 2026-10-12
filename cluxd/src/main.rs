// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]
#![cfg_attr(test, allow(dead_code))]

#[allow(unsafe_code)]
mod abi;
mod automata;
mod block;
mod boot;
mod event;
mod log;
mod power;
mod psi;
mod quiet;
mod sched;
mod sysfs;

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    use core::sync::atomic::{AtomicBool, Ordering};

    static PANICKING: AtomicBool = AtomicBool::new(false);

    if !PANICKING.swap(true, Ordering::Relaxed) {
        let line = info.location().map_or(0, |at| i64::from(at.line()));
        log::num(log::Level::Error, c"panic at line", line);
    }

    abi::abort()
}

#[cfg(not(test))]
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn main() -> core::ffi::c_int {
    boot::run()
}
