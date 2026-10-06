// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

pub mod gauge;

#[derive(Clone, Copy, Debug, Default)]
pub struct Bus {
    pub cpu: u32,
    pub io: u32,
}
