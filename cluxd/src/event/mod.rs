// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

pub mod slot;

use crate::abi::{self, Epoll, FAULT, Fd, Outcome, READ, Ready};

pub const SIGNAL: u64 = 0;
pub const SCHED: u64 = 1;
pub const BLOCK: u64 = 2;

const BATCH: usize = 8;

#[derive(Clone, Copy, Debug, Default)]
pub struct Wake {
    pub stop: bool,
    fired: u32,
    faulted: u32,
}

impl Wake {
    pub fn fired(self, token: u64) -> bool {
        Self::bit(token).is_some_and(|bit| self.fired & bit != 0)
    }

    pub fn faulted(self, token: u64) -> bool {
        Self::bit(token).is_some_and(|bit| self.faulted & bit != 0)
    }

    fn bit(token: u64) -> Option<u32> {
        1_u32.checked_shl(u32::try_from(token).ok()?)
    }
}

#[derive(Debug)]
pub struct Reactor {
    epoll: Epoll,
    signals: Fd,
    ready: [Ready; BATCH],
}

impl Reactor {
    pub fn open() -> Outcome<Self> {
        let epoll = Epoll::open()?;
        let signals = abi::signals()?;
        epoll.add(&signals, SIGNAL, READ)?;
        Ok(Self {
            epoll,
            signals,
            ready: [Ready::default(); BATCH],
        })
    }

    pub const fn epoll(&self) -> &Epoll {
        &self.epoll
    }

    pub fn wait(&mut self, timeout_ms: i32) -> Outcome<Wake> {
        let count = self.epoll.wait(&mut self.ready, timeout_ms)?;
        let mut wake = Wake::default();

        for event in self.ready.iter().take(count) {
            if event.token == SIGNAL {
                let _ = abi::drain(&self.signals)?;

                wake.stop = true;
            } else if let Some(bit) = Wake::bit(event.token) {
                wake.fired |= bit;

                if event.flags & FAULT != 0 {
                    wake.faulted |= bit;
                }
            }
        }

        Ok(wake)
    }
}
