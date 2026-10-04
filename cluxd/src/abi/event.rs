// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use super::fault::{Fault, Outcome, check};
use super::node::Fd;
use super::raw;

pub use super::raw::Ready;

pub const READ: u32 = raw::FLAG_READ;
pub const PRIORITY: u32 = raw::FLAG_PRIORITY;
pub const FAULT: u32 = raw::FLAG_FAULT;

#[derive(Debug)]
pub struct Epoll(Fd);

impl Epoll {
    pub fn open() -> Outcome<Self> {
        let fd = raw::clux_epoll_open();
        check(fd).map(|_| Self(Fd::adopt(fd)))
    }

    pub fn add(&self, fd: &Fd, token: u64, flags: u32) -> Outcome<()> {
        check(raw::clux_epoll_add(self.0.raw(), fd.raw(), token, flags)).map(|_| ())
    }

    pub fn del(&self, fd: &Fd) -> Outcome<()> {
        check(raw::clux_epoll_del(self.0.raw(), fd.raw())).map(|_| ())
    }

    pub fn wait(&self, out: &mut [Ready], timeout_ms: i32) -> Outcome<usize> {
        let cap = u32::try_from(out.len()).unwrap_or(u32::MAX);
        let got = unsafe { raw::clux_epoll_wait(self.0.raw(), out.as_mut_ptr(), cap, timeout_ms) };
        usize::try_from(check(got)?).map_err(|_| Fault::Parse)
    }
}

pub fn signals() -> Outcome<Fd> {
    let fd = raw::clux_signal_open();
    check(fd).map(|_| Fd::adopt(fd))
}

pub fn drain(fd: &Fd) -> Outcome<u32> {
    check(raw::clux_signal_drain(fd.raw()))
}
