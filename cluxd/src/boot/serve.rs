// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use super::gate::millis;
use crate::abi::{self, Epoll, Outcome};
use crate::block;
use crate::charger;
use crate::cpufreq;
use crate::display;
use crate::event::slot::{Cause, Slot, Unit};
use crate::event::{self, Reactor, Wake};
use crate::log;
use crate::mem;
use crate::net;
use crate::psi::Bus;
use crate::quiet;
use crate::rcu;
use crate::reclaim;
use crate::sched;
use crate::touch;
use crate::vm;
use crate::writeback;
use crate::zram;

const MAX_WAIT_MS: i32 = 60_000;

#[derive(Debug)]
struct Governors {
    mem: Slot<mem::Governor>,
    display: Slot<display::Governor>,
    touch: Slot<touch::Governor>,
    charger: Slot<charger::Governor>,
    block: Slot<block::Governor>,
    sched: Slot<sched::Governor>,
    cpufreq: Slot<cpufreq::Governor>,
    rcu: Slot<rcu::Governor>,
    net: Slot<net::Governor>,
    quiet: Slot<quiet::Governor>,
    vm: Slot<vm::Governor>,
    writeback: Slot<writeback::Governor>,
    reclaim: Slot<reclaim::Governor>,
    zram: Slot<zram::Governor>,
}

impl Governors {
    const fn new() -> Self {
        Self {
            mem: Slot::new(event::MEM),
            display: Slot::new(event::DISPLAY),
            touch: Slot::new(event::TOUCH),
            charger: Slot::new(event::CHARGER),
            block: Slot::new(event::BLOCK),
            sched: Slot::new(event::SCHED),
            cpufreq: Slot::new(event::CPUFREQ),
            rcu: Slot::new(event::RCU),
            net: Slot::new(event::NET),
            quiet: Slot::new(event::QUIET),
            vm: Slot::new(event::VM),
            writeback: Slot::new(event::WRITEBACK),
            reclaim: Slot::new(event::RECLAIM),
            zram: Slot::new(event::ZRAM),
        }
    }

    const fn deadlines(&self) -> [Option<u64>; 14] {
        [
            self.mem.deadline(),
            self.display.deadline(),
            self.touch.deadline(),
            self.charger.deadline(),
            self.block.deadline(),
            self.sched.deadline(),
            self.cpufreq.deadline(),
            self.rcu.deadline(),
            self.net.deadline(),
            self.quiet.deadline(),
            self.vm.deadline(),
            self.writeback.deadline(),
            self.reclaim.deadline(),
            self.zram.deadline(),
        ]
    }

    const fn dead(&self) -> bool {
        self.block.dead()
            && self.sched.dead()
            && self.cpufreq.dead()
            && self.rcu.dead()
            && self.net.dead()
            && self.quiet.dead()
            && self.vm.dead()
            && self.writeback.dead()
            && self.reclaim.dead()
            && self.zram.dead()
    }

    fn dispatch(&mut self, wake: Wake, epoll: &Epoll, now_us: u64, bus: &mut Bus) {
        let held = bus.touch_until;

        route(&mut self.mem, wake, epoll, now_us, bus);
        route(&mut self.touch, wake, epoll, now_us, bus);
        route(&mut self.block, wake, epoll, now_us, bus);
        route(&mut self.sched, wake, epoll, now_us, bus);

        let nudge = interaction(held, bus.touch_until, now_us);

        self.mem.pump(epoll, now_us, bus, Cause::Timer);
        self.display.pump(epoll, now_us, bus, Cause::Timer);
        self.touch.pump(epoll, now_us, bus, Cause::Timer);
        self.charger.pump(epoll, now_us, bus, Cause::Timer);
        self.block.pump(epoll, now_us, bus, Cause::Timer);
        self.sched.pump(epoll, now_us, bus, nudge);
        self.cpufreq.pump(epoll, now_us, bus, nudge);
        self.rcu.pump(epoll, now_us, bus, nudge);
        self.net.pump(epoll, now_us, bus, Cause::Timer);
        self.quiet.pump(epoll, now_us, bus, Cause::Timer);
        self.vm.pump(epoll, now_us, bus, Cause::Timer);
        self.writeback.pump(epoll, now_us, bus, Cause::Timer);
        self.reclaim.pump(epoll, now_us, bus, Cause::Timer);
        self.zram.pump(epoll, now_us, bus, Cause::Timer);
    }

    fn retire(&mut self) {
        self.mem.retire();
        self.display.retire();
        self.touch.retire();
        self.charger.retire();
        self.block.retire();
        self.sched.retire();
        self.cpufreq.retire();
        self.rcu.retire();
        self.net.retire();
        self.quiet.retire();
        self.vm.retire();
        self.writeback.retire();
        self.reclaim.retire();
        self.zram.retire();
    }
}

const fn interaction(before_us: u64, after_us: u64, now_us: u64) -> Cause {
    if after_us > now_us && before_us <= now_us {
        Cause::Trigger
    } else {
        Cause::Timer
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
    fn a_first_touch_after_idle_nudges_the_governors() {
        assert_eq!(
            interaction(0, after(2_000_000), NOW_US),
            Cause::Trigger,
            "interaction began"
        );
        assert_eq!(
            interaction(NOW_US.saturating_sub(1), after(2_000_000), NOW_US),
            Cause::Trigger,
            "previous hold already over"
        );
    }

    #[test]
    fn a_continuing_touch_does_not_nudge_again() {
        assert_eq!(
            interaction(after(1_000_000), after(2_000_000), NOW_US),
            Cause::Timer,
            "hold renewed while still held"
        );
    }

    #[test]
    fn no_touch_means_no_nudge() {
        assert_eq!(interaction(0, 0, NOW_US), Cause::Timer, "never touched");
        assert_eq!(
            interaction(NOW_US.saturating_sub(5), NOW_US.saturating_sub(5), NOW_US),
            Cause::Timer,
            "hold expired without a new event"
        );
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
