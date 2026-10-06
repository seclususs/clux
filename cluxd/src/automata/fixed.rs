// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

pub fn at<T: Copy + Default, const N: usize>(table: &[T; N], rung: usize) -> T {
    table
        .get(rung)
        .or_else(|| table.last())
        .copied()
        .unwrap_or_default()
}

pub fn narrow(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

pub fn signed(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

pub fn ratio(num: u64, den: u64, scale: u64) -> u32 {
    narrow(num.saturating_mul(scale).checked_div(den).unwrap_or(0))
}

pub fn scaled(value: u32, per_mille: u32) -> u32 {
    ratio(u64::from(value), 1000, u64::from(per_mille))
}

pub fn floor_to(value: u32, step: u32) -> u32 {
    value
        .checked_div(step)
        .map_or(value, |count| count.saturating_mul(step))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_clamps_to_the_last_entry() {
        assert_eq!(at(&[5_u32, 6, 7], 1), 6);
        assert_eq!(at(&[5_u32, 6, 7], 9), 7);
        assert_eq!(at::<u32, 0>(&[], 0), 0);
    }

    #[test]
    fn narrow_saturates() {
        assert_eq!(narrow(7), 7);
        assert_eq!(narrow(u64::MAX), u32::MAX);
    }

    #[test]
    fn ratio_guards_zero_denominator() {
        assert_eq!(ratio(5, 0, 1000), 0);
        assert_eq!(ratio(1, 4, 1000), 250);
        assert_eq!(ratio(u64::MAX, 1, 1000), u32::MAX);
    }

    #[test]
    fn scaled_and_floor() {
        assert_eq!(scaled(384, 700), 268);
        assert_eq!(floor_to(2_720_000, 50_000), 2_700_000);
        assert_eq!(floor_to(9, 0), 9);
    }
}
