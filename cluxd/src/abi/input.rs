// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use super::fault::{Fault, Outcome, check};
use super::node::Fd;
use super::raw;

pub const CAP_POINTER: u32 = raw::INPUT_POINTER;
pub const CAP_DIRECT: u32 = raw::INPUT_DIRECT;
pub const CAP_MULTITOUCH: u32 = raw::INPUT_MULTITOUCH;
pub const CAP_ABSOLUTE: u32 = raw::INPUT_ABSOLUTE;
pub const CAP_CONTACT: u32 = raw::INPUT_CONTACT;

pub fn input_open(path: &CStr) -> Outcome<Fd> {
    let fd = unsafe { raw::clux_input_open(path.as_ptr()) };
    check(fd).map(|_| Fd::adopt(fd))
}

pub fn input_caps(fd: &Fd) -> Outcome<u32> {
    check(raw::clux_input_caps(fd.raw()))
}

pub fn input_drain(fd: &Fd) -> Outcome<u32> {
    check(raw::clux_input_drain(fd.raw())).map_err(|_| Fault::Parse)
}
