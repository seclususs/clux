// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use super::gate::millis;
use crate::abi::{self, Outcome};
use crate::block;
use crate::event::slot::{Cause, Slot};
use crate::event::{self, Reactor};
use crate::log;
use crate::psi::Bus;
use crate::sched;

const MAX_WAIT_MS: i32 = 10_000;

fn timeout(now_us: u64, due: [Option<u64>; 2]) -> i32 {
    due.into_iter().flatten().min().map_or(MAX_WAIT_MS, |at| {
        millis(at.saturating_sub(now_us)).min(MAX_WAIT_MS)
    })
}

pub fn run(reactor: &mut Reactor) -> Outcome<()> {
    let mut bus = Bus::default();
    let mut block = Slot::<block::Governor>::new(event::BLOCK);
    let mut sched = Slot::<sched::Governor>::new(event::SCHED);

    loop {
        let wake = reactor.wait(timeout(abi::now_us(), [block.deadline(), sched.deadline()]))?;
        if wake.stop {
            break;
        }

        let now = abi::now_us();
        let epoll = reactor.epoll();

        if wake.faulted(block.token()) {
            block.collapse(epoll, now);
        } else if wake.fired(block.token()) {
            block.pump(epoll, now, &mut bus, Cause::Trigger);
        }

        if wake.faulted(sched.token()) {
            sched.collapse(epoll, now);
        } else if wake.fired(sched.token()) {
            sched.pump(epoll, now, &mut bus, Cause::Trigger);
        }

        block.pump(epoll, now, &mut bus, Cause::Timer);
        sched.pump(epoll, now, &mut bus, Cause::Timer);

        if block.dead() && sched.dead() {
            log::warn(c"no governor can run on this device");
            break;
        }
    }

    block.retire();
    sched.retire();
    Ok(())
}
