// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use super::fault::{Fault, Outcome, check};
use super::raw;

#[derive(Clone, Copy, Debug)]
pub enum Mode {
    Read,
    Write,
    ReadWrite,
}

impl Mode {
    const fn code(self) -> u32 {
        match self {
            Self::Read => raw::NODE_READ,
            Self::Write => raw::NODE_WRITE,
            Self::ReadWrite => raw::NODE_RDWR,
        }
    }
}

#[derive(Debug)]
pub struct Fd(i32);

impl Fd {
    pub(super) const fn adopt(fd: i32) -> Self {
        Self(fd)
    }

    pub const fn raw(&self) -> i32 {
        self.0
    }

    pub fn read(&self, buf: &mut [u8]) -> Outcome<usize> {
        let got = unsafe { raw::clux_node_read(self.0, buf.as_mut_ptr().cast(), buf.len()) };
        size(got)
    }

    pub fn write(&self, buf: &[u8]) -> Outcome<usize> {
        let done = unsafe { raw::clux_node_write(self.0, buf.as_ptr().cast(), buf.len()) };
        size(done)
    }
}

impl Drop for Fd {
    fn drop(&mut self) {
        unsafe { raw::clux_node_close(self.0) }
    }
}

fn size(ret: i32) -> Outcome<usize> {
    usize::try_from(check(ret)?).map_err(|_| Fault::Parse)
}

fn descriptor(ret: i32) -> Outcome<Fd> {
    check(ret).map(|_| Fd::adopt(ret))
}

pub fn open(path: &CStr, mode: Mode) -> Outcome<Fd> {
    descriptor(unsafe { raw::clux_node_open(path.as_ptr(), mode.code()) })
}

pub fn slurp(path: &CStr, buf: &mut [u8]) -> Outcome<usize> {
    size(unsafe { raw::clux_node_slurp(path.as_ptr(), buf.as_mut_ptr().cast(), buf.len()) })
}

pub fn scan(dir: &CStr, buf: &mut [u8]) -> Outcome<usize> {
    size(unsafe { raw::clux_node_scan(dir.as_ptr(), buf.as_mut_ptr().cast(), buf.len()) })
}

pub fn arm(path: &CStr, threshold_us: u32, window_us: u32) -> Outcome<Fd> {
    descriptor(unsafe { raw::clux_psi_arm(path.as_ptr(), threshold_us, window_us) })
}
