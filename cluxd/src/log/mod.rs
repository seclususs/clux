// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use crate::abi;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

const FLOOR: u32 = if cfg!(debug_assertions) {
    Level::Info.code()
} else {
    Level::Error.code()
};

const fn enabled(level: Level) -> bool {
    level.code() >= FLOOR
}

pub fn say(level: Level, msg: &CStr) {
    if enabled(level) {
        abi::note(level.code(), msg);
    }
}

pub fn num(level: Level, msg: &CStr, value: i64) {
    if enabled(level) {
        abi::note_num(level.code(), msg, value);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_are_always_recorded() {
        assert!(enabled(Level::Error), "error level must never be filtered");
    }

    #[test]
    fn release_keeps_only_errors() {
        assert_eq!(
            enabled(Level::Info),
            cfg!(debug_assertions),
            "info is a debug-only level"
        );
        assert_eq!(
            enabled(Level::Warn),
            cfg!(debug_assertions),
            "warn is a debug-only level"
        );
    }

    #[test]
    fn levels_map_to_android_priorities() {
        assert_eq!(Level::Info.code(), 4, "ANDROID_LOG_INFO");
        assert_eq!(Level::Warn.code(), 5, "ANDROID_LOG_WARN");
        assert_eq!(Level::Error.code(), 6, "ANDROID_LOG_ERROR");
    }
}
