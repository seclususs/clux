// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

pub mod gauge;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Screen {
    #[default]
    On,
    Off,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Bus {
    pub cpu: u32,
    pub io: u32,
    pub mem: u32,
    pub screen: Screen,
    pub touch_until: u64,
}

pub const MEM_TIGHT: u32 = 2;
