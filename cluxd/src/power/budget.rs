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
    cpu: Option<i32>,
    heat: Option<i32>,
}

impl Budget {
    pub const fn new() -> Self {
        Self {
            thermal: Ladder::new(THERMAL_ENTER, THERMAL_LEAVE, THERMAL_HOLD_US),
            battery: Ladder::new(BATTERY_ENTER, BATTERY_LEAVE, BATTERY_HOLD_US),
            cpu: None,
            heat: None,
        }
    }

    pub fn feed(&mut self, reading: Reading, now_us: u64) {
        self.cpu = reading.cpu.or(self.cpu);
        self.heat = reading.battery.or(self.heat);

        if let Some(cpu) = self.cpu {
            let margin = self.heat.map_or(0, |heat| {
                heat.saturating_sub(MARGIN_START).clamp(0, MARGIN_SPAN)
            });

            let mut input = u32::try_from(cpu.saturating_add(margin).max(0)).unwrap_or(0);

            if self.heat.is_some_and(|heat| heat >= BATTERY_LIMIT) {
                input = input.max(CRITICAL_INPUT);
            }

            let _ = self.thermal.step(input, now_us);
        }

        if let Some(level) = reading.level {
            let _ = self
                .battery
                .step(100_u32.saturating_sub(level.min(100)), now_us);
        }
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
}
