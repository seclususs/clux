// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use super::fixed::signed;

#[derive(Clone, Copy, Debug)]
pub struct Envelope {
    level: u32,
    slope: i32,
    attack: u32,
    release: u32,
    snap: u32,
}

impl Envelope {
    pub fn new(attack: u32, release: u32, snap: u32) -> Self {
        Self {
            level: 0,
            slope: 0,
            attack: attack.max(1),
            release: release.max(1),
            snap,
        }
    }

    pub const fn level(&self) -> u32 {
        self.level
    }

    pub const fn slope(&self) -> i32 {
        self.slope
    }

    pub const fn reset(&mut self, value: u32) {
        self.level = value;
        self.slope = 0;
    }

    pub fn feed(&mut self, value: u32) -> u32 {
        let prev = self.level;

        self.level = if value >= prev {
            let gap = value.saturating_sub(prev);
            if gap >= self.snap {
                value
            } else {
                prev.saturating_add(gap.div_ceil(self.attack))
            }
        } else {
            prev.saturating_sub(prev.saturating_sub(value).div_ceil(self.release))
        };

        let delta = signed(self.level).saturating_sub(signed(prev));
        self.slope = self
            .slope
            .saturating_add(delta.saturating_sub(self.slope).saturating_div(2));

        self.level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attack_is_faster_than_release() {
        let mut up = Envelope::new(2, 8, 500);
        let mut down = Envelope::new(2, 8, 500);
        down.reset(400);
        let rise = up.feed(400);
        let fall = 400 - down.feed(0);
        assert!(rise > fall);
        assert_eq!(rise, 200);
        assert_eq!(fall, 50);
    }

    #[test]
    fn converges_exactly() {
        let mut env = Envelope::new(2, 8, 500);
        for _ in 0..64 {
            let _ = env.feed(300);
        }
        assert_eq!(env.level(), 300);
        for _ in 0..200 {
            let _ = env.feed(0);
        }
        assert_eq!(env.level(), 0);
    }

    #[test]
    fn snaps_on_large_jump() {
        let mut env = Envelope::new(2, 8, 250);
        assert_eq!(env.feed(600), 600);
        assert!(env.slope() > 0);
    }

    #[test]
    fn slope_tracks_direction() {
        let mut env = Envelope::new(2, 4, 900);
        for step in 1..=5 {
            let _ = env.feed(step * 100);
        }
        assert!(env.slope() > 0);
        for _ in 0..12 {
            let _ = env.feed(0);
        }
        assert!(env.slope() <= 0);
    }
}
