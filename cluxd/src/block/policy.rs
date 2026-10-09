// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use crate::automata::cadence::Cadence;
use crate::automata::envelope::Envelope;
use crate::automata::fixed::at;
use crate::automata::ladder::Ladder;

use super::stat::Metrics;

const PRESSURE_ENTER: [u32; 4] = [0, 50, 120, 220];
const PRESSURE_LEAVE: [u32; 4] = [0, 30, 90, 170];
const PRESSURE_HOLD_US: u64 = 2_000_000;
const PATTERN_ENTER: [u32; 4] = [0, 100, 300, 600];
const PATTERN_LEAVE: [u32; 4] = [0, 60, 220, 500];
const PATTERN_HOLD_US: u64 = 3_000_000;
const SHARE: [u32; 4] = [1000, 750, 500, 250];
const READ_AHEAD: [u32; 4] = [128, 256, 512, 1024];
const PACE_MS: [u32; 4] = [100, 250, 1000, 5000];
const PACE_CEILING: [usize; 4] = [3, 2, 1, 0];
const PATTERN_CEILING: [usize; 4] = [3, 2, 1, 1];
const PACE_PATIENCE: u32 = 2;
const SLOW_MS: u32 = 40;
const STALLED_MS: u32 = 100;
const SLOW_FLOOR: u32 = 120;
const STALLED_FLOOR: u32 = 220;

#[derive(Clone, Copy, Debug)]
pub struct Input {
    pub io: u32,
    pub metrics: Metrics,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    pub read_ahead: u32,
    pub share: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct Decision {
    pub plan: Plan,
    pub next_us: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct Policy {
    pressure: Ladder<4>,
    pattern: Ladder<4>,
    smooth: Envelope,
    cadence: Cadence<4>,
}

impl Policy {
    pub fn new() -> Self {
        Self {
            pressure: Ladder::new(PRESSURE_ENTER, PRESSURE_LEAVE, PRESSURE_HOLD_US),
            pattern: Ladder::new(PATTERN_ENTER, PATTERN_LEAVE, PATTERN_HOLD_US),
            smooth: Envelope::new(2, 2, 1000),
            cadence: Cadence::new(PACE_MS, PACE_PATIENCE),
        }
    }

    pub fn interval_us(&self) -> u64 {
        self.cadence.interval_us()
    }

    pub fn step(&mut self, input: Input, now_us: u64, urgent: bool) -> Decision {
        let _ = self
            .pressure
            .step(input.io.max(slo_floor(input.metrics.latency_ms)), now_us);

        let _ = self
            .pattern
            .step(self.smooth.feed(input.metrics.sequential), now_us);

        if urgent {
            self.cadence.boost();
        }

        let rung = self.pressure.rung();

        Decision {
            plan: Plan {
                read_ahead: at(&READ_AHEAD, self.pattern.rung()),
                share: at(&SHARE, rung),
            },
            next_us: self
                .cadence
                .next_us(at(&PACE_CEILING, rung).min(at(&PATTERN_CEILING, self.pattern.rung()))),
        }
    }
}

const fn slo_floor(latency_ms: u32) -> u32 {
    if latency_ms >= STALLED_MS {
        STALLED_FLOOR
    } else if latency_ms >= SLOW_MS {
        SLOW_FLOOR
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: u64 = 1_000_000;

    fn metrics(sequential: u32, latency_ms: u32) -> Metrics {
        Metrics {
            sequential,
            latency_ms,
        }
    }

    #[test]
    fn idle_disk_keeps_full_queue_and_small_readahead() {
        let mut policy = Policy::new();
        let plan = policy
            .step(
                Input {
                    io: 0,
                    metrics: metrics(0, 0),
                },
                0,
                false,
            )
            .plan;
        assert_eq!(
            plan,
            Plan {
                read_ahead: 128,
                share: 1000
            }
        );
    }

    #[test]
    fn congestion_shrinks_the_queue() {
        let mut policy = Policy::new();
        let plan = policy
            .step(
                Input {
                    io: 300,
                    metrics: metrics(0, 0),
                },
                0,
                false,
            )
            .plan;
        assert_eq!(plan.share, 250);
    }

    #[test]
    fn slow_requests_shrink_the_queue_without_pressure() {
        let mut policy = Policy::new();
        let slow = policy
            .step(
                Input {
                    io: 0,
                    metrics: metrics(0, 45),
                },
                0,
                false,
            )
            .plan;
        assert_eq!(slow.share, 500);
        let mut policy = Policy::new();
        let stalled = policy
            .step(
                Input {
                    io: 0,
                    metrics: metrics(0, 150),
                },
                0,
                false,
            )
            .plan;
        assert_eq!(stalled.share, 250);
    }

    #[test]
    fn sustained_streaming_raises_readahead_then_decays() {
        let mut policy = Policy::new();
        let mut plan = Plan {
            read_ahead: 0,
            share: 0,
        };
        for tick in 0..8 {
            plan = policy
                .step(
                    Input {
                        io: 60,
                        metrics: metrics(900, 5),
                    },
                    tick * S,
                    false,
                )
                .plan;
        }
        assert_eq!(plan.read_ahead, 1024);
        for tick in 8..40 {
            plan = policy
                .step(
                    Input {
                        io: 0,
                        metrics: metrics(0, 0),
                    },
                    tick * S,
                    false,
                )
                .plan;
        }
        assert_eq!(plan.read_ahead, 128);
        assert_eq!(plan.share, 1000);
    }

    #[test]
    fn detected_streaming_is_tracked_at_a_faster_pace() {
        let mut policy = Policy::new();
        let idle = policy
            .step(
                Input {
                    io: 0,
                    metrics: metrics(0, 0),
                },
                0,
                false,
            )
            .next_us;
        let mut tracked = idle;
        for tick in 1..4 {
            tracked = policy
                .step(
                    Input {
                        io: 0,
                        metrics: metrics(900, 5),
                    },
                    tick * S,
                    false,
                )
                .next_us;
        }
        assert_eq!(idle, 5_000_000);
        assert_eq!(tracked, 250_000);
    }

    #[test]
    fn idle_disk_is_polled_slowly() {
        let mut policy = Policy::new();
        let idle = policy
            .step(
                Input {
                    io: 0,
                    metrics: metrics(0, 0),
                },
                0,
                false,
            )
            .next_us;
        let busy = policy
            .step(
                Input {
                    io: 300,
                    metrics: metrics(0, 0),
                },
                S,
                false,
            )
            .next_us;
        assert_eq!(idle, 5_000_000);
        assert_eq!(busy, 100_000);
    }
}
