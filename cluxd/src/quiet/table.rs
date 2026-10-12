// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

#[derive(Clone, Copy, Debug)]
pub struct Rule {
    pub path: &'static CStr,
    pub floor: u32,
}

pub const RULES: [Rule; 2] = [
    Rule {
        path: c"/sys/module/binder/parameters/debug_mask",
        floor: 0,
    },
    Rule {
        path: c"/sys/module/lowmemorykiller/parameters/debug_level",
        floor: 0,
    },
];
