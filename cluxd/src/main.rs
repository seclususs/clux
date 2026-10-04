// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]
#![cfg_attr(test, allow(dead_code))]

#[allow(unsafe_code)]
mod abi;
mod automata;
mod log;
mod sysfs;

#[cfg(not(test))]
#[panic_handler]
const fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[cfg(not(test))]
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
const extern "C" fn main() -> core::ffi::c_int {
    0
}
