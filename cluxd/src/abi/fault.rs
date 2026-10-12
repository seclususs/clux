// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

pub const EPERM: i32 = 1;
pub const ENOENT: i32 = 2;
pub const EACCES: i32 = 13;
pub const ENODEV: i32 = 19;
pub const EROFS: i32 = 30;
pub const EWOULDBLOCK: i32 = 11;
pub const EOPNOTSUPP: i32 = 95;

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
            Self::Absent | Self::Errno(EPERM | ENOENT | EACCES | ENODEV | EROFS | EOPNOTSUPP)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_kernel_support_is_permanent() {
        for errno in [EPERM, ENOENT, EACCES, ENODEV, EROFS, EOPNOTSUPP] {
            assert!(
                Fault::errno(-errno).fatal(),
                "errno {errno} must stop the retry loop"
            );
        }
        assert!(
            Fault::Absent.fatal(),
            "absent node must stop the retry loop"
        );
    }

    #[test]
    fn transient_faults_keep_retrying() {
        for errno in [4, 5, 11, 12, 16, 110] {
            assert!(
                !Fault::errno(-errno).fatal(),
                "errno {errno} must stay retryable"
            );
        }
        assert!(!Fault::Parse.fatal(), "parse failure stays retryable");
    }

    #[test]
    fn only_would_block_means_contended() {
        assert!(Fault::errno(-EWOULDBLOCK).contended(), "flock contention");
        assert!(
            !Fault::errno(-EPERM).contended(),
            "permission is not contention"
        );
    }

    #[test]
    fn codes_are_positive_errno_or_negative_markers() {
        assert_eq!(Fault::errno(-EOPNOTSUPP).code(), 95, "errno code");
        assert_eq!(Fault::Parse.code(), -1, "parse marker");
        assert_eq!(Fault::Absent.code(), -2, "absent marker");
    }
}
