// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use crate::automata::ladder::Ladder;

const SECOND: u64 = 1_000_000;
const THERMAL_ENTER: [u32; 4] = [0, 475, 525, 575];
const THERMAL_LEAVE: [u32; 4] = [0, 445, 495, 545];
const THERMAL_SCALE: [u32; 4] = [1000, 700, 400, 150];
const THERMAL_HOLD_US: u64 = 20 * SECOND;
const BATTERY_ENTER: [u32; 4] = [0, 50, 75, 88];
const BATTERY_LEAVE: [u32; 4] = [0, 47, 72, 85];
const BATTERY_CEILING: [usize; 4] = [4, 3, 2, 1];
const BATTERY_HOLD_US: u64 = 30 * SECOND;
const MARGIN_START: i32 = 355;
const MARGIN_SPAN: i32 = 50;
const BATTERY_LIMIT: i32 = 405;
const CRITICAL_INPUT: u32 = 575;
const CPU_TTL_US: u64 = 60 * SECOND;
const HEAT_TTL_US: u64 = 120 * SECOND;
const LEVEL_TTL_US: u64 = 600 * SECOND;

#[derive(Clone, Copy, Debug)]
struct Aged<T> {
    value: T,
    at_us: u64,
}

fn renew<T: Copy>(held: Option<Aged<T>>, reading: Option<T>, now_us: u64) -> Option<Aged<T>> {
    reading
        .map(|value| Aged {
            value,
            at_us: now_us,
        })
        .or(held)
}

fn fresh<T: Copy>(held: Option<Aged<T>>, now_us: u64, ttl_us: u64) -> Option<T> {
    held.filter(|aged| now_us.saturating_sub(aged.at_us) <= ttl_us)
        .map(|aged| aged.value)
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Reading {
    pub cpu: Option<i32>,
    pub battery: Option<i32>,
    pub level: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Verdict {
    pub scale: u32,
    pub ceiling: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct Budget {
    thermal: Ladder<4>,
    battery: Ladder<4>,
    cpu: Option<Aged<i32>>,
    heat: Option<Aged<i32>>,
    level: Option<Aged<u32>>,
}

impl Budget {
    pub const fn new() -> Self {
        Self {
            thermal: Ladder::new(THERMAL_ENTER, THERMAL_LEAVE, THERMAL_HOLD_US),
            battery: Ladder::new(BATTERY_ENTER, BATTERY_LEAVE, BATTERY_HOLD_US),
            cpu: None,
            heat: None,
            level: None,
        }
    }

    pub fn lost(&self, now_us: u64) -> bool {
        self.cpu.is_some() && fresh(self.cpu, now_us, CPU_TTL_US).is_none()
    }

    pub fn feed(&mut self, reading: Reading, now_us: u64) {
        self.cpu = renew(self.cpu, reading.cpu, now_us);
        self.heat = renew(self.heat, reading.battery, now_us);
        self.level = renew(self.level, reading.level, now_us);

        let heat = fresh(self.heat, now_us, HEAT_TTL_US);
        let input = fresh(self.cpu, now_us, CPU_TTL_US).map_or(0, |cpu| {
            let margin = heat.map_or(0, |heat| {
                heat.saturating_sub(MARGIN_START).clamp(0, MARGIN_SPAN)
            });

            let input = u32::try_from(cpu.saturating_add(margin).max(0)).unwrap_or(0);
            if heat.is_some_and(|heat| heat >= BATTERY_LIMIT) {
                input.max(CRITICAL_INPUT)
            } else {
                input
            }
        });

        let _ = self.thermal.step(input, now_us);

        let used = fresh(self.level, now_us, LEVEL_TTL_US)
            .map_or(0, |level| 100_u32.saturating_sub(level.min(100)));

        let _ = self.battery.step(used, now_us);
    }

    pub fn verdict(&self) -> Verdict {
        Verdict {
            scale: THERMAL_SCALE
                .get(self.thermal.rung())
                .copied()
                .unwrap_or(1000),
            ceiling: BATTERY_CEILING
                .get(self.battery.rung())
                .copied()
                .unwrap_or(4),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: u64 = 1_000_000;

    fn reading(cpu: i32, battery: i32, level: u32) -> Reading {
        Reading {
            cpu: Some(cpu),
            battery: Some(battery),
            level: Some(level),
        }
    }

    #[test]
    fn cool_device_has_full_budget() {
        let mut budget = Budget::new();
        budget.feed(reading(400, 300, 90), 0);
        assert_eq!(
            budget.verdict(),
            Verdict {
                scale: 1000,
                ceiling: 4
            }
        );
    }

    #[test]
    fn heat_steps_up_at_once_and_down_after_hold() {
        let mut budget = Budget::new();
        budget.feed(reading(500, 300, 90), 0);
        assert_eq!(budget.verdict().scale, 700);
        budget.feed(reading(540, 300, 90), S);
        assert_eq!(budget.verdict().scale, 400);
        budget.feed(reading(400, 300, 90), 2 * S);
        assert_eq!(budget.verdict().scale, 400);
        budget.feed(reading(400, 300, 90), 23 * S);
        assert_eq!(budget.verdict().scale, 1000);
    }

    #[test]
    fn warm_battery_shifts_the_cpu_setpoint() {
        let mut budget = Budget::new();
        budget.feed(reading(430, 400, 90), 0);
        assert_eq!(budget.verdict().scale, 700);
    }

    #[test]
    fn battery_hard_limit_forces_critical() {
        let mut budget = Budget::new();
        budget.feed(reading(300, 410, 90), 0);
        assert_eq!(budget.verdict().scale, 150);
    }

    #[test]
    fn low_charge_caps_the_regime() {
        let mut budget = Budget::new();
        budget.feed(reading(400, 300, 40), 0);
        assert_eq!(budget.verdict().ceiling, 3);
        budget.feed(reading(400, 300, 20), S);
        assert_eq!(budget.verdict().ceiling, 2);
        budget.feed(reading(400, 300, 8), 2 * S);
        assert_eq!(budget.verdict().ceiling, 1);
    }

    #[test]
    fn missing_sensors_keep_last_known_state() {
        let mut budget = Budget::new();
        budget.feed(Reading::default(), 0);
        assert_eq!(
            budget.verdict(),
            Verdict {
                scale: 1000,
                ceiling: 4
            }
        );
        budget.feed(reading(540, 300, 90), S);
        budget.feed(Reading::default(), 2 * S);
        assert_eq!(budget.verdict().scale, 400);
    }

    #[test]
    fn a_lost_sensor_expires_instead_of_throttling_forever() {
        let mut budget = Budget::new();
        budget.feed(reading(540, 300, 90), S);
        assert_eq!(budget.verdict().scale, 400);
        budget.feed(Reading::default(), 30 * S);
        assert_eq!(budget.verdict().scale, 400);
        assert!(!budget.lost(30 * S));
        budget.feed(Reading::default(), 70 * S);
        assert!(budget.lost(70 * S));
        assert_eq!(budget.verdict().scale, 400);
        budget.feed(Reading::default(), 91 * S);
        assert_eq!(budget.verdict().scale, 1000);
    }

    #[test]
    fn a_sensor_that_never_existed_is_not_lost() {
        let mut budget = Budget::new();
        for tick in 0..10 {
            budget.feed(Reading::default(), tick * 100 * S);
            assert!(!budget.lost(tick * 100 * S));
        }
        assert_eq!(budget.verdict().scale, 1000);
    }

    #[test]
    fn stale_battery_heat_stops_forcing_the_critical_rung() {
        let mut budget = Budget::new();
        budget.feed(reading(300, 410, 90), 0);
        assert_eq!(budget.verdict().scale, 150);
        for tick in 1..=4 {
            budget.feed(
                Reading {
                    cpu: Some(300),
                    battery: None,
                    level: Some(90),
                },
                tick * 60 * S,
            );
        }
        assert_eq!(budget.verdict().scale, 1000);
    }

    #[test]
    fn stale_charge_level_releases_the_battery_ceiling() {
        let mut budget = Budget::new();
        budget.feed(reading(400, 300, 8), 0);
        assert_eq!(budget.verdict().ceiling, 1);
        budget.feed(Reading::default(), 700 * S);
        budget.feed(Reading::default(), 740 * S);
        assert_eq!(budget.verdict().ceiling, 4);
    }
}
