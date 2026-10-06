// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

const DIGITS: usize = 20;

#[derive(Clone, Copy, Debug)]
pub struct Decimal {
    buf: [u8; DIGITS + 1],
    start: usize,
}

impl Decimal {
    pub fn new(mut value: u64) -> Self {
        let mut buf = [0_u8; DIGITS + 1];
        let mut start = DIGITS;

        loop {
            start = start.saturating_sub(1);
            let digit = u8::try_from(value % 10).unwrap_or(0);

            if let Some(cell) = buf.get_mut(start) {
                *cell = b'0'.saturating_add(digit);
            }

            value /= 10;
            if value == 0 || start == 0 {
                break;
            }
        }

        if let Some(cell) = buf.get_mut(DIGITS) {
            *cell = b'\n';
        }

        Self { buf, start }
    }

    pub fn digits(&self) -> &[u8] {
        self.buf.get(self.start..DIGITS).unwrap_or_default()
    }

    pub fn line(&self) -> &[u8] {
        self.buf.get(self.start..).unwrap_or_default()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Path<const N: usize> {
    buf: [u8; N],
    len: usize,
}

impl<const N: usize> Path<N> {
    pub const fn new() -> Self {
        Self {
            buf: [0; N],
            len: 0,
        }
    }

    pub fn push(mut self, part: &[u8]) -> Option<Self> {
        let end = self.len.checked_add(part.len())?;
        if end >= N {
            return None;
        }

        self.buf.get_mut(self.len..end)?.copy_from_slice(part);
        self.len = end;
        Some(self)
    }

    pub fn push_uint(self, value: u64) -> Option<Self> {
        self.push(Decimal::new(value).digits())
    }

    pub fn cstr(&self) -> Option<&CStr> {
        CStr::from_bytes_until_nul(&self.buf).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_renders_without_leading_zeros() {
        assert_eq!(Decimal::new(0).digits(), b"0");
        assert_eq!(Decimal::new(8_000_000).line(), b"8000000\n");
        assert_eq!(Decimal::new(u64::MAX).digits(), b"18446744073709551615");
    }

    #[test]
    fn path_builds_nul_terminated_strings() {
        let path = Path::<48>::new()
            .push(b"/sys/class/thermal/thermal_zone")
            .and_then(|p| p.push_uint(12))
            .and_then(|p| p.push(b"/type"))
            .expect("fits");
        assert_eq!(
            path.cstr().expect("cstr").to_bytes(),
            b"/sys/class/thermal/thermal_zone12/type"
        );
    }

    #[test]
    fn path_rejects_overflow() {
        assert!(Path::<8>::new().push(b"12345678").is_none());
        assert!(Path::<8>::new().push(b"1234567").is_some());
    }
}
