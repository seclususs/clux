// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::{c_char, c_void};

pub const VERSION: u32 = 1;
pub const NODE_READ: u32 = 0;
pub const NODE_WRITE: u32 = 1;
pub const NODE_RDWR: u32 = 2;
pub const FLAG_READ: u32 = 1;
pub const FLAG_PRIORITY: u32 = 2;
pub const FLAG_FAULT: u32 = 4;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Ready {
    pub token: u64,
    pub flags: u32,
    pub reserved: u32,
}

const _: () = assert!(
    size_of::<Ready>() == 16,
    "Ready must match clux::event::Ready"
);

#[cfg_attr(not(test), link(name = "clux", kind = "static"))]
#[cfg_attr(target_os = "android", link(name = "log"))]
#[cfg_attr(not(test), link(name = "c"))]
unsafe extern "C" {
    pub(super) safe fn clux_abi() -> u32;
    pub(super) safe fn clux_clock_us() -> u64;
    pub(super) fn clux_node_open(path: *const c_char, mode: u32) -> i32;
    pub(super) fn clux_node_read(fd: i32, buf: *mut c_void, cap: usize) -> i32;
    pub(super) fn clux_node_write(fd: i32, buf: *const c_void, len: usize) -> i32;
    pub(super) fn clux_node_close(fd: i32);
    pub(super) fn clux_node_slurp(path: *const c_char, buf: *mut c_void, cap: usize) -> i32;
    pub(super) fn clux_node_scan(dir: *const c_char, buf: *mut c_void, cap: usize) -> i32;
    pub(super) fn clux_psi_arm(path: *const c_char, threshold_us: u32, window_us: u32) -> i32;
    pub(super) safe fn clux_epoll_open() -> i32;
    pub(super) safe fn clux_epoll_add(ep: i32, fd: i32, token: u64, flags: u32) -> i32;
    pub(super) safe fn clux_epoll_del(ep: i32, fd: i32) -> i32;
    pub(super) fn clux_epoll_wait(ep: i32, out: *mut Ready, cap: u32, timeout_ms: i32) -> i32;
    pub(super) safe fn clux_signal_open() -> i32;
    pub(super) safe fn clux_signal_drain(fd: i32) -> i32;
    pub(super) fn clux_prop_get(key: *const c_char, out: *mut c_char, cap: usize) -> i32;
    pub(super) fn clux_log(level: u32, msg: *const c_char);
    pub(super) fn clux_log_num(level: u32, msg: *const c_char, value: i64);
    pub(super) safe fn clux_proc_root() -> i32;
    pub(super) safe fn clux_proc_detach() -> i32;
    pub(super) fn clux_proc_lock(path: *const c_char) -> i32;
    pub(super) safe fn clux_proc_shield() -> i32;
    pub(super) safe fn clux_proc_limit() -> i32;
    pub(super) safe fn clux_proc_cores() -> i32;
    pub(super) safe fn clux_proc_pin(mask: u64) -> i32;
    pub(super) safe fn clux_proc_realtime(priority: u32) -> i32;
    pub(super) safe fn clux_proc_clamp(util_max: u32) -> i32;
    pub(super) safe fn clux_proc_ioprio() -> i32;
    pub(super) safe fn clux_proc_slack(nanos: u64) -> i32;
    pub(super) safe fn clux_proc_lockmem() -> i32;
    pub(super) safe fn clux_proc_abort() -> !;
}
