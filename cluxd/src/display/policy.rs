// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use crate::psi::Screen;

const LIT_INTERVAL_US: u64 = 2_000_000;
const DARK_INTERVAL_US: u64 = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Backlight,
    Blank,
}

pub const fn lit(source: Source, level: u64) -> bool {
    match source {
        Source::Backlight => level > 0,
        Source::Blank => level == 0,
    }
}

pub const fn screen(lit: bool) -> Screen {
    if lit { Screen::On } else { Screen::Off }
}

pub const fn next_us(screen: Screen) -> u64 {
    match screen {
        Screen::On => LIT_INTERVAL_US,
        Screen::Off => DARK_INTERVAL_US,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_backlight_above_zero_means_the_panel_is_lit() {
        assert!(lit(Source::Backlight, 1), "dimmest level");
        assert!(lit(Source::Backlight, 255), "full level");
        assert!(!lit(Source::Backlight, 0), "backlight off");
    }

    #[test]
    fn only_the_unblanked_level_means_the_panel_is_lit() {
        assert!(lit(Source::Blank, 0), "FB_BLANK_UNBLANK");
        for level in 1..=4 {
            assert!(!lit(Source::Blank, level), "blank level {level}");
        }
    }

    #[test]
    fn the_screen_follows_the_panel() {
        assert_eq!(screen(true), Screen::On, "lit panel");
        assert_eq!(screen(false), Screen::Off, "dark panel");
    }

    #[test]
    fn a_dark_screen_is_watched_for_waking_at_least_as_often() {
        assert!(
            next_us(Screen::Off) <= next_us(Screen::On),
            "waking must be noticed quickly"
        );
    }
}
