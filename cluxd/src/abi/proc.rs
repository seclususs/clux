// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use super::fault::{Outcome, check};
use super::node::Fd;
use super::raw;

fn unit(ret: i32) -> Outcome<()> {
    check(ret).map(|_| ())
}

pub fn root() -> Outcome<()> {
    unit(raw::clux_proc_root())
}

pub fn detach() -> Outcome<()> {
    unit(raw::clux_proc_detach())
}

pub fn lock(path: &CStr) -> Outcome<Fd> {
    let fd = unsafe { raw::clux_proc_lock(path.as_ptr()) };
    check(fd).map(|_| Fd::adopt(fd))
}

pub fn shield() -> Outcome<()> {
    unit(raw::clux_proc_shield())
}

pub fn limit() -> Outcome<()> {
    unit(raw::clux_proc_limit())
}

pub fn cores() -> Outcome<u32> {
    check(raw::clux_proc_cores())
}

pub fn pin(mask: u64) -> Outcome<()> {
    unit(raw::clux_proc_pin(mask))
}

pub fn realtime(priority: u32) -> Outcome<()> {
    unit(raw::clux_proc_realtime(priority))
}

pub fn clamp(util_max: u32) -> Outcome<()> {
    unit(raw::clux_proc_clamp(util_max))
}

pub fn ioprio() -> Outcome<()> {
    unit(raw::clux_proc_ioprio())
}

pub fn slack(nanos: u64) -> Outcome<()> {
    unit(raw::clux_proc_slack(nanos))
}

pub fn lockmem() -> Outcome<()> {
    unit(raw::clux_proc_lockmem())
}

pub fn abort() -> ! {
    raw::clux_proc_abort()
}
