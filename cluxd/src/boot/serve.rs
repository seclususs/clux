// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use super::gate::millis;
use crate::abi::{self, Epoll, Outcome};
use crate::block;
use crate::event::slot::{Cause, Slot, Unit};
use crate::event::{self, Reactor, Wake};
use crate::log;
use crate::psi::Bus;
use crate::quiet;
use crate::sched;
use crate::vm;

const MAX_WAIT_MS: i32 = 60_000;

#[derive(Debug)]
struct Governors {
    block: Slot<block::Governor>,
    sched: Slot<sched::Governor>,
    quiet: Slot<quiet::Governor>,
    vm: Slot<vm::Governor>,
}

impl Governors {
    const fn new() -> Self {
        Self {
            block: Slot::new(event::BLOCK),
            sched: Slot::new(event::SCHED),
            quiet: Slot::new(event::QUIET),
            vm: Slot::new(event::VM),
        }
    }

    const fn deadlines(&self) -> [Option<u64>; 4] {
        [
            self.block.deadline(),
            self.sched.deadline(),
            self.quiet.deadline(),
            self.vm.deadline(),
        ]
    }

    const fn dead(&self) -> bool {
        self.block.dead() && self.sched.dead() && self.quiet.dead() && self.vm.dead()
    }

    fn dispatch(&mut self, wake: Wake, epoll: &Epoll, now_us: u64, bus: &mut Bus) {
        route(&mut self.block, wake, epoll, now_us, bus);
        route(&mut self.sched, wake, epoll, now_us, bus);

        self.block.pump(epoll, now_us, bus, Cause::Timer);
        self.sched.pump(epoll, now_us, bus, Cause::Timer);
        self.quiet.pump(epoll, now_us, bus, Cause::Timer);
        self.vm.pump(epoll, now_us, bus, Cause::Timer);
    }

    fn retire(&mut self) {
        self.block.retire();
        self.sched.retire();
        self.quiet.retire();
        self.vm.retire();
    }
}

fn route<U: Unit>(slot: &mut Slot<U>, wake: Wake, epoll: &Epoll, now_us: u64, bus: &mut Bus) {
    if wake.faulted(slot.token()) {
        slot.collapse(epoll, now_us, bus);
    } else if wake.fired(slot.token()) {
        slot.pump(epoll, now_us, bus, Cause::Trigger);
    }
}

fn timeout<const N: usize>(now_us: u64, due: [Option<u64>; N]) -> i32 {
    due.into_iter().flatten().min().map_or(MAX_WAIT_MS, |at| {
        millis(at.saturating_sub(now_us)).min(MAX_WAIT_MS)
    })
}

pub fn run(reactor: &mut Reactor) -> Outcome<()> {
    let mut bus = Bus::default();
    let mut governors = Governors::new();

    let outcome = cycle(reactor, &mut bus, &mut governors);

    governors.retire();
    outcome
}

fn cycle(reactor: &mut Reactor, bus: &mut Bus, governors: &mut Governors) -> Outcome<()> {
    loop {
        let wake = reactor.wait(timeout(abi::now_us(), governors.deadlines()))?;
        if wake.stop {
            return Ok(());
        }

        governors.dispatch(wake, reactor.epoll(), abi::now_us(), bus);

        if governors.dead() {
            log::error(c"no governor can run on this device");
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW_US: u64 = 5_000_000;

    fn after(us: u64) -> u64 {
        NOW_US.saturating_add(us)
    }

    #[test]
    fn idle_loop_sleeps_for_the_cap() {
        assert_eq!(
            timeout(NOW_US, [None, None, None]),
            MAX_WAIT_MS,
            "no deadline pending"
        );
    }

    #[test]
    fn nearest_deadline_wins() {
        assert_eq!(
            timeout(NOW_US, [Some(after(5_000_000)), Some(after(250_000)), None]),
            250,
            "earliest of the pending deadlines"
        );
    }

    #[test]
    fn partial_milliseconds_round_up() {
        assert_eq!(
            timeout(NOW_US, [None, Some(after(1_001)), None]),
            2,
            "never wake before the deadline"
        );
    }

    #[test]
    fn overdue_deadline_polls_without_sleeping() {
        assert_eq!(
            timeout(NOW_US, [Some(NOW_US.saturating_sub(5)), None, None]),
            0,
            "deadline already passed"
        );
    }

    #[test]
    fn far_deadline_is_capped() {
        assert_eq!(
            timeout(NOW_US, [None, None, Some(after(900_000_000))]),
            MAX_WAIT_MS,
            "long backoff must not stretch the wait past the cap"
        );
    }
}
