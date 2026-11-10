// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use crate::psi::Screen;
use crate::sysfs::lexer;
use crate::vm::policy::Verdict;

const DARK_US: u64 = 30_000_000;
const DARK_PLUGGED_US: u64 = 10_000_000;
const CPU_IDLE: u32 = 30;
const IO_IDLE: u32 = 30;
const WASTE_FLOOR: u64 = 4 << 20;
const COOLDOWN_US: u64 = 1_800_000_000;
const COOLDOWN_CAP_US: u64 = 14_400_000_000;
const WATCH_US: u64 = 15_000_000;
const LAZY_US: u64 = 60_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stat {
    pub stored: u64,
    pub used: u64,
    pub compacted: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct Input {
    pub mem: u32,
    pub cpu: u32,
    pub io: u32,
    pub screen: Screen,
    pub plugged: bool,
    pub engaged: bool,
    pub swap: Verdict,
    pub stat: Option<Stat>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Wait(u64),
    Compact,
}

#[derive(Clone, Copy, Debug)]
pub struct Policy {
    dark_since: Option<u64>,
    ready_us: u64,
    cooldown_us: u64,
}

pub fn parse(text: &[u8]) -> Option<Stat> {
    let mut words = lexer::words(text);
    let mut fields = [0_u64; 7];

    for field in &mut fields {
        *field = lexer::uint(words.next()?)?;
    }

    let [_, stored, used, _, _, _, compacted] = fields;
    Some(Stat {
        stored,
        used,
        compacted,
    })
}

const fn worth(stat: Stat) -> bool {
    stat.used.saturating_sub(stat.stored) >= WASTE_FLOOR
}

const fn quiet(input: &Input) -> bool {
    input.mem == 0 && input.cpu < CPU_IDLE && input.io < IO_IDLE && !input.engaged
}

impl Policy {
    pub const fn new() -> Self {
        Self {
            dark_since: None,
            ready_us: 0,
            cooldown_us: COOLDOWN_US,
        }
    }

    fn dark(&mut self, input: &Input, now_us: u64) -> bool {
        if !matches!(input.screen, Screen::Off) {
            self.dark_since = None;
            return false;
        }

        let since = *self.dark_since.get_or_insert(now_us);
        let needed = if input.plugged {
            DARK_PLUGGED_US
        } else {
            DARK_US
        };

        now_us.saturating_sub(since) >= needed
    }

    pub fn step(&mut self, input: &Input, now_us: u64) -> Action {
        let dark = self.dark(input, now_us);
        let wait = Action::Wait(if matches!(input.screen, Screen::Off) {
            WATCH_US
        } else {
            LAZY_US
        });

        let eligible = dark
            && quiet(input)
            && input.swap == Verdict::Apply
            && now_us >= self.ready_us
            && input.stat.is_some_and(worth);

        if eligible { Action::Compact } else { wait }
    }

    pub fn done(&mut self, gained: u64, now_us: u64) {
        self.cooldown_us = if gained == 0 {
            self.cooldown_us.saturating_mul(2).min(COOLDOWN_CAP_US)
        } else {
            COOLDOWN_US
        };

        self.ready_us = now_us.saturating_add(self.cooldown_us);
    }

    pub const fn watch_us() -> u64 {
        WATCH_US
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: u64 = 1_000_000;
    const FRAGMENTED: Stat = Stat {
        stored: 100 << 20,
        used: 120 << 20,
        compacted: 7,
    };

    fn idle() -> Input {
        Input {
            mem: 0,
            cpu: 0,
            io: 0,
            screen: Screen::Off,
            plugged: false,
            engaged: false,
            swap: Verdict::Apply,
            stat: Some(FRAGMENTED),
        }
    }

    fn settled(policy: &mut Policy, input: &Input) -> Action {
        let _ = policy.step(input, 100 * S);
        policy.step(input, 100 * S + DARK_US)
    }

    #[test]
    fn mm_stat_is_read_from_the_documented_columns() {
        let text = b"  104857600 83886080 125829120        0 130023424      512        9        0        0\n";
        assert_eq!(
            parse(text),
            Some(Stat {
                stored: 83_886_080,
                used: 125_829_120,
                compacted: 9
            }),
            "current layout"
        );
    }

    #[test]
    fn the_older_seven_column_layout_is_still_understood() {
        let text = b"4096 2048 4096 0 4096 1 3\n";
        assert_eq!(
            parse(text),
            Some(Stat {
                stored: 2048,
                used: 4096,
                compacted: 3
            }),
            "seven columns"
        );
    }

    #[test]
    fn a_short_or_garbled_line_is_rejected() {
        assert_eq!(parse(b"1 2 3 4 5 6\n"), None, "six columns");
        assert_eq!(parse(b"1 2 x 4 5 6 7\n"), None, "non numeric column");
        assert_eq!(parse(b""), None, "empty");
    }

    #[test]
    fn an_idle_dark_device_with_waste_is_compacted() {
        let mut policy = Policy::new();
        assert_eq!(settled(&mut policy, &idle()), Action::Compact, "all clear");
    }

    #[test]
    fn the_screen_must_stay_dark_for_a_while_first() {
        let mut policy = Policy::new();
        assert_eq!(
            policy.step(&idle(), 100 * S),
            Action::Wait(WATCH_US),
            "just went dark"
        );
        assert_eq!(
            policy.step(&idle(), 100 * S + DARK_US.saturating_sub(1)),
            Action::Wait(WATCH_US),
            "a moment short"
        );
    }

    #[test]
    fn external_power_shortens_the_wait() {
        let mut policy = Policy::new();
        let plugged = Input {
            plugged: true,
            ..idle()
        };
        let _ = policy.step(&plugged, 100 * S);
        assert_eq!(
            policy.step(&plugged, 100 * S + DARK_PLUGGED_US),
            Action::Compact,
            "plugged in"
        );
    }

    #[test]
    fn a_lit_screen_never_compacts_and_restarts_the_wait() {
        let mut policy = Policy::new();
        assert_eq!(settled(&mut policy, &idle()), Action::Compact, "dark");
        let lit = Input {
            screen: Screen::On,
            ..idle()
        };
        assert_eq!(policy.step(&lit, 300 * S), Action::Wait(LAZY_US), "lit");
        assert_eq!(
            policy.step(&idle(), 301 * S),
            Action::Wait(WATCH_US),
            "dark again must wait afresh"
        );
    }

    #[test]
    fn any_activity_postpones_compaction() {
        let busy = [
            Input { mem: 1, ..idle() },
            Input {
                cpu: CPU_IDLE,
                ..idle()
            },
            Input {
                io: IO_IDLE,
                ..idle()
            },
            Input {
                engaged: true,
                ..idle()
            },
        ];

        for input in busy {
            let mut policy = Policy::new();
            assert_eq!(
                settled(&mut policy, &input),
                Action::Wait(WATCH_US),
                "{input:?}"
            );
        }
    }

    #[test]
    fn only_a_zram_only_swap_is_compacted() {
        for swap in [Verdict::Release, Verdict::Hold] {
            let mut policy = Policy::new();
            let input = Input { swap, ..idle() };
            assert_eq!(
                settled(&mut policy, &input),
                Action::Wait(WATCH_US),
                "{swap:?}"
            );
        }
    }

    #[test]
    fn a_tidy_pool_is_left_alone() {
        let tidy = Stat {
            stored: 100 << 20,
            used: (100 << 20) + WASTE_FLOOR - 1,
            compacted: 0,
        };
        let mut policy = Policy::new();
        let input = Input {
            stat: Some(tidy),
            ..idle()
        };
        assert_eq!(settled(&mut policy, &input), Action::Wait(WATCH_US), "tidy");
        let unreadable = Input {
            stat: None,
            ..idle()
        };
        assert_eq!(
            settled(&mut policy, &unreadable),
            Action::Wait(WATCH_US),
            "no statistics"
        );
    }

    #[test]
    fn a_compaction_starts_a_cooldown() {
        let mut policy = Policy::new();
        assert_eq!(settled(&mut policy, &idle()), Action::Compact, "first");
        let now = 200 * S;
        policy.done(12, now);
        assert_eq!(
            policy.step(&idle(), now + COOLDOWN_US.saturating_sub(1)),
            Action::Wait(WATCH_US),
            "cooling down"
        );
        assert_eq!(
            policy.step(&idle(), now + COOLDOWN_US),
            Action::Compact,
            "cooled down"
        );
    }

    #[test]
    fn a_futile_compaction_backs_off_up_to_a_limit() {
        let mut policy = Policy::new();
        let _ = policy.step(&idle(), 0);
        for round in 1..=6_u32 {
            policy.done(0, 0);
            let expected = COOLDOWN_US
                .saturating_mul(2_u64.saturating_pow(round))
                .min(COOLDOWN_CAP_US);
            assert_eq!(policy.ready_us, expected, "round {round}");
        }
    }

    #[test]
    fn a_useful_compaction_resets_the_backoff() {
        let mut policy = Policy::new();
        policy.done(0, 0);
        policy.done(0, 0);
        policy.done(5, 0);
        assert_eq!(policy.ready_us, COOLDOWN_US, "back to the base cooldown");
    }

    #[test]
    fn the_watch_interval_is_shorter_than_the_lazy_one() {
        assert!(
            Policy::watch_us() < LAZY_US,
            "dark devices are watched closely"
        );
    }
}
