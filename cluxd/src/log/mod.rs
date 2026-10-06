// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use crate::abi;

#[derive(Clone, Copy, Debug)]
pub enum Level {
    Info,
    Warn,
    Error,
}

impl Level {
    const fn code(self) -> u32 {
        match self {
            Self::Info => 4,
            Self::Warn => 5,
            Self::Error => 6,
        }
    }
}

pub fn say(level: Level, msg: &CStr) {
    abi::note(level.code(), msg);
}

pub fn num(level: Level, msg: &CStr, value: i64) {
    abi::note_num(level.code(), msg, value);
}

pub fn info(msg: &CStr) {
    say(Level::Info, msg);
}

pub fn warn(msg: &CStr) {
    say(Level::Warn, msg);
}

pub fn error(msg: &CStr) {
    say(Level::Error, msg);
}
