// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

mod event;
mod fault;
mod node;
mod proc;
mod raw;

use core::ffi::CStr;

pub use event::{Epoll, FAULT, PRIORITY, READ, Ready, drain, signals};
pub use fault::{Fault, Outcome};
pub use node::{Fd, Mode, arm, debugfs, devno, open, scan, slurp};
#[cfg_attr(test, allow(unused_imports))]
pub use proc::abort;
pub use proc::{
    clamp, cores, detach, ioprio, limit, lock, lockmem, pin, realtime, root, shield, slack,
};

use fault::check;

pub fn compatible() -> bool {
    raw::clux_abi() == raw::VERSION
}

pub fn now_us() -> u64 {
    raw::clux_clock_us()
}

pub fn prop(key: &CStr, buf: &mut [u8]) -> Outcome<usize> {
    let got = unsafe { raw::clux_prop_get(key.as_ptr(), buf.as_mut_ptr().cast(), buf.len()) };
    usize::try_from(check(got)?).map_err(|_| Fault::Parse)
}

#[cfg(not(test))]
pub fn note(level: u32, msg: &CStr) {
    unsafe { raw::clux_log(level, msg.as_ptr()) }
}

#[cfg(not(test))]
pub fn note_num(level: u32, msg: &CStr, value: i64) {
    unsafe { raw::clux_log_num(level, msg.as_ptr(), value) }
}

#[cfg(test)]
#[allow(clippy::missing_const_for_fn)]
pub fn note(_level: u32, _msg: &CStr) {}

#[cfg(test)]
#[allow(clippy::missing_const_for_fn)]
pub fn note_num(_level: u32, _msg: &CStr, _value: i64) {}
