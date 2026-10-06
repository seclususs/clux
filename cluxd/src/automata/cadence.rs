// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::cmp::Ordering;

#[derive(Clone, Copy, Debug)]
pub struct Cadence<const N: usize> {
    rung: usize,
    calm: u32,
    patience: u32,
    millis: [u32; N],
}

impl<const N: usize> Cadence<N> {
    pub const fn new(millis: [u32; N], patience: u32) -> Self {
        Self {
            rung: N.saturating_sub(1),
            calm: 0,
            patience,
            millis,
        }
    }

    pub const fn boost(&mut self) {
        self.rung = 0;
        self.calm = 0;
    }

    pub fn next_us(&mut self, ceiling: usize) -> u64 {
        let ceiling = ceiling.min(N.saturating_sub(1));

        match ceiling.cmp(&self.rung) {
            Ordering::Less => {
                self.rung = ceiling;
                self.calm = 0;
            }

            Ordering::Greater => {
                self.calm = self.calm.saturating_add(1);
                if self.calm >= self.patience {
                    self.rung = self.rung.saturating_add(1);
                    self.calm = 0;
                }
            }

            Ordering::Equal => self.calm = 0,
        }

        self.interval_us()
    }

    pub fn interval_us(&self) -> u64 {
        let millis = self.millis.get(self.rung).copied().unwrap_or(1000);
        u64::from(millis).saturating_mul(1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cadence() -> Cadence<4> {
        Cadence::new([100, 500, 2000, 8000], 2)
    }

    #[test]
    fn starts_relaxed_and_speeds_up_at_once() {
        let mut c = cadence();
        assert_eq!(c.interval_us(), 8_000_000);
        assert_eq!(c.next_us(0), 100_000);
    }

    #[test]
    fn slows_down_one_rung_per_patience() {
        let mut c = cadence();
        let _ = c.next_us(0);
        assert_eq!(c.next_us(3), 100_000);
        assert_eq!(c.next_us(3), 500_000);
        assert_eq!(c.next_us(3), 500_000);
        assert_eq!(c.next_us(3), 2_000_000);
    }

    #[test]
    fn boost_resets_and_ceiling_is_clamped() {
        let mut c = cadence();
        let _ = c.next_us(2);
        c.boost();
        assert_eq!(c.interval_us(), 100_000);
        assert_eq!(c.next_us(99), 100_000);
    }
}
