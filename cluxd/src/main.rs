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
mod sched;
mod sysfs;

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    abi::abort()
}

#[cfg(not(test))]
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn main() -> core::ffi::c_int {
    boot::run()
}
