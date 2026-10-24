// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use super::gate::millis;
use crate::abi::{self, Outcome};
use crate::block;
use crate::event::slot::{Cause, Slot};
use crate::event::{self, Reactor};
use crate::log;
use crate::psi::Bus;
use crate::quiet;
use crate::sched;

const MAX_WAIT_MS: i32 = 60_000;

fn timeout<const N: usize>(now_us: u64, due: [Option<u64>; N]) -> i32 {
    due.into_iter().flatten().min().map_or(MAX_WAIT_MS, |at| {
        millis(at.saturating_sub(now_us)).min(MAX_WAIT_MS)
    })
}

pub fn run(reactor: &mut Reactor) -> Outcome<()> {
    let mut bus = Bus::default();
    let mut block = Slot::<block::Governor>::new(event::BLOCK);
    let mut sched = Slot::<sched::Governor>::new(event::SCHED);
    let mut quiet = Slot::<quiet::Governor>::new(event::QUIET);

    let outcome = cycle(reactor, &mut bus, &mut block, &mut sched, &mut quiet);

    block.retire();
    sched.retire();
    quiet.retire();
    outcome
}

fn cycle(
    reactor: &mut Reactor,
    bus: &mut Bus,
    block: &mut Slot<block::Governor>,
    sched: &mut Slot<sched::Governor>,
    quiet: &mut Slot<quiet::Governor>,
) -> Outcome<()> {
    loop {
        let due = [block.deadline(), sched.deadline(), quiet.deadline()];
        let wake = reactor.wait(timeout(abi::now_us(), due))?;
        if wake.stop {
            return Ok(());
        }

        let now = abi::now_us();
        let epoll = reactor.epoll();

        if wake.faulted(block.token()) {
            block.collapse(epoll, now, bus);
        } else if wake.fired(block.token()) {
            block.pump(epoll, now, bus, Cause::Trigger);
        }

        if wake.faulted(sched.token()) {
            sched.collapse(epoll, now, bus);
        } else if wake.fired(sched.token()) {
            sched.pump(epoll, now, bus, Cause::Trigger);
        }

        block.pump(epoll, now, bus, Cause::Timer);
        sched.pump(epoll, now, bus, Cause::Timer);
        quiet.pump(epoll, now, bus, Cause::Timer);

        if block.dead() && sched.dead() && quiet.dead() {
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
