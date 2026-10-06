// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use crate::abi::{Epoll, Fault, Fd, Outcome, PRIORITY};
use crate::log::{self, Level};
use crate::psi::Bus;

const COOLDOWN_US: u64 = 5_000_000;

pub trait Watch {
    fn attach(&self, fd: &Fd, token: u64) -> Outcome<()>;
    fn detach(&self, fd: &Fd);
}

impl Watch for Epoll {
    fn attach(&self, fd: &Fd, token: u64) -> Outcome<()> {
        self.add(fd, token, PRIORITY)
    }

    fn detach(&self, fd: &Fd) {
        let _ = self.del(fd);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    Trigger,
    Timer,
}

pub trait Unit: Sized {
    const STARTED: &'static CStr;
    const FAILED: &'static CStr;

    fn spawn(now_us: u64) -> Outcome<Self>;
    fn watch(&self) -> Option<&Fd>;
    fn tick(&mut self, now_us: u64, bus: &mut Bus, cause: Cause) -> Outcome<u64>;
    fn retire(&mut self);
}

#[derive(Clone, Copy, Debug)]
enum Phase {
    Down(u64),
    Up(u64),
    Dead,
}

#[derive(Debug)]
pub struct Slot<U: Unit> {
    token: u64,
    phase: Phase,
    unit: Option<U>,
}

impl<U: Unit> Slot<U> {
    pub const fn new(token: u64) -> Self {
        Self {
            token,
            phase: Phase::Down(0),
            unit: None,
        }
    }

    pub const fn token(&self) -> u64 {
        self.token
    }

    pub const fn dead(&self) -> bool {
        matches!(self.phase, Phase::Dead)
    }

    pub const fn deadline(&self) -> Option<u64> {
        match self.phase {
            Phase::Down(at) | Phase::Up(at) => Some(at),
            Phase::Dead => None,
        }
    }

    pub fn pump<W: Watch>(&mut self, watch: &W, now_us: u64, bus: &mut Bus, cause: Cause) {
        match self.phase {
            Phase::Down(at) if now_us >= at => self.revive(watch, now_us, bus),
            Phase::Up(at) if cause == Cause::Trigger || now_us >= at => {
                self.drive(watch, now_us, bus, cause);
            }
            Phase::Dead | Phase::Down(_) | Phase::Up(_) => {}
        }
    }

    pub fn collapse<W: Watch>(&mut self, watch: &W, now_us: u64) {
        if self.unit.is_some() {
            self.fail(watch, now_us, Fault::Errno(5));
        }
    }

    pub fn retire(&mut self) {
        if let Some(unit) = self.unit.as_mut() {
            unit.retire();
        }
        self.unit = None;
        self.phase = Phase::Dead;
    }

    fn revive<W: Watch>(&mut self, watch: &W, now_us: u64, bus: &mut Bus) {
        match U::spawn(now_us) {
            Ok(unit) => {
                if let Some(fd) = unit.watch()
                    && watch.attach(fd, self.token).is_err()
                {
                    log::warn(c"event source rejected, falling back to timer");
                }
                self.unit = Some(unit);
                self.phase = Phase::Up(now_us);
                log::info(U::STARTED);
                self.drive(watch, now_us, bus, Cause::Timer);
            }
            Err(fault) => self.reschedule(now_us, fault),
        }
    }

    fn drive<W: Watch>(&mut self, watch: &W, now_us: u64, bus: &mut Bus, cause: Cause) {
        let outcome = self.unit.as_mut().map(|unit| unit.tick(now_us, bus, cause));
        match outcome {
            Some(Ok(next)) => self.phase = Phase::Up(next),
            Some(Err(fault)) => self.fail(watch, now_us, fault),
            None => self.phase = Phase::Down(now_us.saturating_add(COOLDOWN_US)),
        }
    }

    fn fail<W: Watch>(&mut self, watch: &W, now_us: u64, fault: Fault) {
        if let Some(mut unit) = self.unit.take() {
            if let Some(fd) = unit.watch() {
                watch.detach(fd);
            }
            unit.retire();
        }
        self.reschedule(now_us, fault);
    }

    fn reschedule(&mut self, now_us: u64, fault: Fault) {
        log::num(Level::Warn, U::FAILED, fault.code());
        self.phase = if fault.fatal() {
            Phase::Dead
        } else {
            Phase::Down(now_us.saturating_add(COOLDOWN_US))
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Probe<const MODE: u8>;

    impl<const MODE: u8> Unit for Probe<MODE> {
        const STARTED: &'static CStr = c"probe started";
        const FAILED: &'static CStr = c"probe failed";

        fn spawn(_now_us: u64) -> Outcome<Self> {
            match MODE {
                0 => Ok(Self),
                1 => Err(Fault::Errno(5)),
                _ => Err(Fault::Absent),
            }
        }

        fn watch(&self) -> Option<&Fd> {
            None
        }

        fn tick(&mut self, now_us: u64, _bus: &mut Bus, _cause: Cause) -> Outcome<u64> {
            Ok(now_us.saturating_add(1000))
        }

        fn retire(&mut self) {}
    }

    struct Null;

    impl Watch for Null {
        fn attach(&self, _fd: &Fd, _token: u64) -> Outcome<()> {
            Ok(())
        }

        fn detach(&self, _fd: &Fd) {}
    }

    #[test]
    fn spawn_then_tick_schedules_next_deadline() {
        let mut bus = Bus::default();
        let mut slot = Slot::<Probe<0>>::new(1);
        slot.pump(&Null, 100, &mut bus, Cause::Timer);
        assert_eq!(slot.deadline(), Some(1100));
        slot.pump(&Null, 500, &mut bus, Cause::Timer);
        assert_eq!(slot.deadline(), Some(1100));
        slot.pump(&Null, 600, &mut bus, Cause::Trigger);
        assert_eq!(slot.deadline(), Some(1600));
    }

    #[test]
    fn transient_failure_backs_off_then_retries() {
        let mut bus = Bus::default();
        let mut slot = Slot::<Probe<1>>::new(1);
        slot.pump(&Null, 0, &mut bus, Cause::Timer);
        assert_eq!(slot.deadline(), Some(COOLDOWN_US));
        slot.pump(&Null, 1000, &mut bus, Cause::Timer);
        assert_eq!(slot.deadline(), Some(COOLDOWN_US));
        slot.pump(&Null, COOLDOWN_US, &mut bus, Cause::Timer);
        assert_eq!(slot.deadline(), Some(COOLDOWN_US.saturating_mul(2)));
        assert!(!slot.dead());
    }

    #[test]
    fn fatal_failure_disables_the_unit_for_good() {
        let mut bus = Bus::default();
        let mut slot = Slot::<Probe<2>>::new(1);
        slot.pump(&Null, 0, &mut bus, Cause::Timer);
        assert!(slot.dead());
        assert_eq!(slot.deadline(), None);
        slot.pump(&Null, u64::MAX, &mut bus, Cause::Trigger);
        assert!(slot.dead());
    }

    #[test]
    fn collapse_restarts_a_running_unit_after_cooldown() {
        let mut bus = Bus::default();
        let mut slot = Slot::<Probe<0>>::new(1);
        slot.pump(&Null, 0, &mut bus, Cause::Timer);
        slot.collapse(&Null, 10);
        assert_eq!(slot.deadline(), Some(COOLDOWN_US.saturating_add(10)));
        slot.pump(
            &Null,
            COOLDOWN_US.saturating_add(10),
            &mut bus,
            Cause::Timer,
        );
        assert_eq!(slot.deadline(), Some(COOLDOWN_US.saturating_add(1010)));
    }

    #[test]
    fn retire_is_terminal() {
        let mut bus = Bus::default();
        let mut slot = Slot::<Probe<0>>::new(1);
        slot.pump(&Null, 0, &mut bus, Cause::Timer);
        slot.retire();
        assert!(slot.dead());
    }
}
