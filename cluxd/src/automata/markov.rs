// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

#[derive(Clone, Copy, Debug)]
pub struct Markov<const N: usize> {
    counts: [[u8; N]; N],
}

const MIN_SAMPLES: u32 = 3;
const CEILING: u8 = u8::MAX;

impl<const N: usize> Markov<N> {
    pub const fn new() -> Self {
        Self {
            counts: [[0; N]; N],
        }
    }

    pub fn observe(&mut self, from: usize, to: usize) {
        if from == to {
            return;
        }

        let Some(row) = self.counts.get_mut(from) else {
            return;
        };

        let saturated = row.get(to).is_some_and(|&count| count == CEILING);
        if saturated {
            for count in row.iter_mut() {
                *count >>= 1;
            }
        }

        if let Some(count) = row.get_mut(to) {
            *count = count.saturating_add(1);
        }
    }

    pub fn predict(&self, from: usize) -> Option<usize> {
        let row = self.counts.get(from)?;
        let total: u32 = row.iter().map(|&count| u32::from(count)).sum();
        let (best, &top) = row.iter().enumerate().max_by_key(|&(_, count)| *count)?;
        let confident = u32::from(top).saturating_mul(2) >= total;
        (total >= MIN_SAMPLES && confident).then_some(best)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_evidence_before_predicting() {
        let mut m = Markov::<5>::new();
        assert_eq!(m.predict(1), None);
        m.observe(1, 3);
        m.observe(1, 3);
        assert_eq!(m.predict(1), None);
        m.observe(1, 3);
        assert_eq!(m.predict(1), Some(3));
    }

    #[test]
    fn requires_majority_confidence() {
        let mut m = Markov::<5>::new();
        for to in [2, 3, 4, 2, 3, 4] {
            m.observe(0, to);
        }
        assert_eq!(m.predict(0), None);
        for _ in 0..6 {
            m.observe(0, 4);
        }
        assert_eq!(m.predict(0), Some(4));
    }

    #[test]
    fn ignores_self_transitions_and_bad_indices() {
        let mut m = Markov::<3>::new();
        m.observe(1, 1);
        m.observe(9, 0);
        m.observe(0, 9);
        assert_eq!(m.predict(1), None);
    }

    #[test]
    fn adapts_when_pattern_shifts() {
        let mut m = Markov::<4>::new();
        for _ in 0..40 {
            m.observe(0, 1);
        }
        for _ in 0..400 {
            m.observe(0, 2);
        }
        assert_eq!(m.predict(0), Some(2));
    }
}
