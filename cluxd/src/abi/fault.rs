// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

pub const EPERM: i32 = 1;
pub const ENOENT: i32 = 2;
pub const EACCES: i32 = 13;
pub const ENODEV: i32 = 19;
pub const EROFS: i32 = 30;
pub const EWOULDBLOCK: i32 = 11;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    Errno(i32),
    Parse,
    Absent,
}

pub type Outcome<T> = Result<T, Fault>;

impl Fault {
    pub const fn errno(ret: i32) -> Self {
        Self::Errno(ret.saturating_neg())
    }

    pub const fn fatal(self) -> bool {
        matches!(
            self,
            Self::Absent | Self::Errno(EPERM | ENOENT | EACCES | ENODEV | EROFS)
        )
    }

    pub const fn contended(self) -> bool {
        matches!(self, Self::Errno(EWOULDBLOCK))
    }

    pub fn code(self) -> i64 {
        match self {
            Self::Errno(value) => i64::from(value),
            Self::Parse => -1,
            Self::Absent => -2,
        }
    }
}

pub fn check(ret: i32) -> Outcome<u32> {
    u32::try_from(ret).map_err(|_| Fault::errno(ret))
}
