// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

pub const RUNGS: usize = 5;
pub const PACES: usize = 7;

pub const ENTER: [u32; RUNGS] = [0, 30, 90, 180, 320];
pub const LEAVE: [u32; RUNGS] = [0, 20, 65, 140, 250];
pub const HOLD_US: u64 = 1_500_000;

pub const LATENCY: [u32; RUNGS] = [20_000_000, 16_000_000, 12_000_000, 10_000_000, 8_000_000];
pub const WAKEUP: [u32; RUNGS] = [6_500_000, 5_000_000, 3_500_000, 2_500_000, 1_500_000];
pub const MIGRATION: [u32; RUNGS] = [600_000, 500_000, 400_000, 300_000, 200_000];
pub const WALT: [u32; RUNGS] = [10, 14, 22, 30, 40];

pub const PACE_MS: [u32; PACES] = [100, 250, 500, 1000, 2500, 5000, 10_000];
pub const PACE_CEILING: [usize; RUNGS] = [6, 4, 2, 1, 0];
pub const PACE_PATIENCE: u32 = 2;

pub const LATENCY_MIN: u32 = 8_000_000;
pub const LATENCY_SPAN: u32 = 12_000_000;
pub const GRANULARITY_MIN: u32 = 2_500_000;
pub const GRANULARITY_MAX: u32 = 6_500_000;
pub const GRANULARITY_STEP: u32 = 50_000;
pub const GRANULARITY_PERCENT: u64 = 34;
pub const MIGRATION_FLOOR: u32 = 200_000;
pub const VOLATILE_SLOPE: u32 = 120;
pub const IO_FLOOR: u32 = 100;
pub const IO_HEAVY: u32 = 200;
