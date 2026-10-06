// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use crate::abi::{self, Fd, Outcome};
use crate::log::{self, Level};
use crate::sysfs::probe;

const LOCK: &CStr = c"/data/local/tmp/cluxd.lock";
const PRIORITY: u32 = 1;
const UTIL_MAX: u32 = 102;
const SLACK_NS: u64 = 50_000_000;

pub fn claim() -> Outcome<Fd> {
    abi::lock(LOCK)
}

pub fn harden() {
    report(abi::shield(), c"oom shield failed");
    report(abi::limit(), c"resource limits failed");
    if let Some(mask) = probe::cores() {
        report(abi::pin(mask), c"cpu affinity failed");
    }
    report(abi::ioprio(), c"io priority failed");
    report(abi::slack(SLACK_NS), c"timer slack failed");
    report(abi::realtime(PRIORITY), c"realtime policy failed");
    report(abi::clamp(UTIL_MAX), c"utilization clamp failed");
    report(abi::lockmem(), c"memory lock failed");
}

fn report(result: Outcome<()>, what: &CStr) {
    if let Err(fault) = result {
        log::num(Level::Warn, what, fault.code());
    }
}
