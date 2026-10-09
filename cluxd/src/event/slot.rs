// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use crate::abi::{Epoll, Fault, Fd, Outcome, PRIORITY};
use crate::log::{self, Level};
use crate::psi::Bus;

const COOLDOWN_US: u64 = 5_000_000;
const COOLDOWN_CAP_US: u64 = 300_000_000;
const STABLE_US: u64 = 60_000_000;
const BACKOFF_STEPS: u32 = 6;

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
    fn vacate(bus: &mut Bus);
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
    born_us: u64,
    failures: u32,
}

impl<U: Unit> Slot<U> {
    pub const fn new(token: u64) -> Self {
        Self {
            token,
            phase: Phase::Down(0),
            unit: None,
            born_us: 0,
            failures: 0,
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

    pub fn collapse<W: Watch>(&mut self, watch: &W, now_us: u64, bus: &mut Bus) {
        if self.unit.is_some() {
            self.fail(watch, now_us, bus, Fault::Errno(5));
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
                self.born_us = now_us;
                log::info(U::STARTED);
                self.drive(watch, now_us, bus, Cause::Timer);
            }
            Err(fault) => self.reschedule(now_us, fault),
        }
    }

    fn drive<W: Watch>(&mut self, watch: &W, now_us: u64, bus: &mut Bus, cause: Cause) {
        let outcome = self.unit.as_mut().map(|unit| unit.tick(now_us, bus, cause));
        match outcome {
            Some(Ok(next)) => {
                if now_us.saturating_sub(self.born_us) >= STABLE_US {
                    self.failures = 0;
                }
                self.phase = Phase::Up(next);
            }
            Some(Err(fault)) => self.fail(watch, now_us, bus, fault),
            None => self.phase = Phase::Down(now_us.saturating_add(COOLDOWN_US)),
        }
    }

    fn fail<W: Watch>(&mut self, watch: &W, now_us: u64, bus: &mut Bus, fault: Fault) {
        if let Some(mut unit) = self.unit.take() {
            if let Some(fd) = unit.watch() {
                watch.detach(fd);
            }
            unit.retire();
        }
        U::vacate(bus);
        self.reschedule(now_us, fault);
    }

    fn reschedule(&mut self, now_us: u64, fault: Fault) {
        log::num(Level::Warn, U::FAILED, fault.code());
        self.phase = if fault.fatal() {
            Phase::Dead
        } else {
            Phase::Down(now_us.saturating_add(self.backoff_us()))
        };
        self.failures = self.failures.saturating_add(1).min(BACKOFF_STEPS);
    }

    fn backoff_us(&self) -> u64 {
        let factor = 1_u64.checked_shl(self.failures).unwrap_or(u64::MAX);
        COOLDOWN_US.saturating_mul(factor).min(COOLDOWN_CAP_US)
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

        fn vacate(bus: &mut Bus) {
            bus.io = 0;
            bus.cpu = 0;
        }
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
        assert_eq!(slot.deadline(), Some(COOLDOWN_US.saturating_mul(3)));
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
        slot.collapse(&Null, 10, &mut bus);
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

    #[test]
    fn repeated_spawn_failures_back_off_up_to_the_cap() {
        let mut bus = Bus::default();
        let mut slot = Slot::<Probe<1>>::new(1);
        let mut now = 0_u64;
        let mut gaps = [0_u64; 9];

        for gap in &mut gaps {
            slot.pump(&Null, now, &mut bus, Cause::Timer);
            let next = slot.deadline().unwrap();
            *gap = next - now;
            now = next;
        }

        assert_eq!(
            gaps,
            [
                5_000_000,
                10_000_000,
                20_000_000,
                40_000_000,
                80_000_000,
                160_000_000,
                300_000_000,
                300_000_000,
                300_000_000
            ]
        );
    }

    #[test]
    fn a_failing_unit_releases_what_it_published_on_the_bus() {
        let mut bus = Bus { cpu: 700, io: 400 };
        let mut slot = Slot::<Probe<0>>::new(1);
        slot.pump(&Null, 0, &mut bus, Cause::Timer);
        bus.io = 400;
        slot.collapse(&Null, 10, &mut bus);
        assert_eq!((bus.cpu, bus.io), (0, 0));
    }

    #[test]
    fn a_stable_run_clears_the_backoff() {
        let mut bus = Bus::default();
        let mut slot = Slot::<Probe<0>>::new(1);
        slot.pump(&Null, 0, &mut bus, Cause::Timer);
        slot.collapse(&Null, 10, &mut bus);
        let revived = COOLDOWN_US.saturating_add(10);
        slot.pump(&Null, revived, &mut bus, Cause::Timer);
        slot.collapse(&Null, revived.saturating_add(1), &mut bus);
        assert_eq!(
            slot.deadline(),
            Some(
                revived
                    .saturating_add(1)
                    .saturating_add(COOLDOWN_US.saturating_mul(2))
            )
        );

        let again = revived
            .saturating_add(1)
            .saturating_add(COOLDOWN_US.saturating_mul(2));
        slot.pump(&Null, again, &mut bus, Cause::Timer);
        let stable = again.saturating_add(STABLE_US);
        slot.pump(&Null, stable, &mut bus, Cause::Timer);
        slot.collapse(&Null, stable, &mut bus);
        assert_eq!(slot.deadline(), Some(stable.saturating_add(COOLDOWN_US)));
    }
}
