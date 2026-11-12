// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use super::policy::{BBR, SLOW_START_OFF, congestion, fastopen, qdisc};
use crate::abi::{self, Fault, Fd, Outcome};
use crate::event::slot::{Cause, Unit};
use crate::psi::Bus;
use crate::sysfs::knob::Knob;
use crate::sysfs::text::Text;

const CONGESTION: &CStr = c"/proc/sys/net/ipv4/tcp_congestion_control";
const AVAILABLE: &CStr = c"/proc/sys/net/ipv4/tcp_available_congestion_control";
const QDISC: &CStr = c"/proc/sys/net/core/default_qdisc";
const FASTOPEN: &CStr = c"/proc/sys/net/ipv4/tcp_fastopen";
const SLOW_START: &CStr = c"/proc/sys/net/ipv4/tcp_slow_start_after_idle";
const LIST_CAP: usize = 192;

#[derive(Debug)]
pub struct Governor {
    congestion: Option<Text>,
    qdisc: Option<Text>,
    fastopen: Knob,
    slow_start: Knob,
    applied: bool,
}

impl Governor {
    fn tune_congestion(&mut self) -> bool {
        let Some(node) = self.congestion.as_mut() else {
            return false;
        };

        let mut buf = [0_u8; LIST_CAP];
        let available = abi::slurp(AVAILABLE, &mut buf)
            .ok()
            .and_then(|got| buf.get(..got))
            .unwrap_or_default();

        if let Some(name) = congestion(node.origin(), available) {
            return node.set(name);
        }

        false
    }

    fn tune_qdisc(&mut self, switched: bool) {
        let Some(node) = self.qdisc.as_mut() else {
            return;
        };

        let algorithm = if switched {
            BBR
        } else {
            self.congestion.as_ref().map_or(&b""[..], Text::origin)
        };

        for name in qdisc(node.origin(), algorithm) {
            if node.set(name) {
                break;
            }
        }
    }

    fn apply(&mut self) {
        self.slow_start.set(SLOW_START_OFF);

        if let Some(origin) = self.fastopen.origin() {
            self.fastopen.set(fastopen(origin));
        }

        let switched = self.tune_congestion();
        self.tune_qdisc(switched);
    }
}

impl Unit for Governor {
    const STARTED: &'static CStr = c"network governor online";
    const FAILED: &'static CStr = c"network governor stopped, fault";

    fn spawn(_now_us: u64) -> Outcome<Self> {
        let governor = Self {
            congestion: Text::open(CONGESTION),
            qdisc: Text::open(QDISC),
            fastopen: Knob::open(FASTOPEN),
            slow_start: Knob::open(SLOW_START),
            applied: false,
        };

        let present = governor.congestion.is_some()
            || governor.qdisc.is_some()
            || governor.fastopen.present()
            || governor.slow_start.present();

        if present {
            Ok(governor)
        } else {
            Err(Fault::Absent)
        }
    }

    fn watch(&self) -> Option<&Fd> {
        None
    }

    fn tick(&mut self, _now_us: u64, _bus: &mut Bus, _cause: Cause) -> Outcome<u64> {
        if !self.applied {
            self.apply();
            self.applied = true;
        }

        Ok(u64::MAX)
    }

    fn retire(&mut self) {
        self.slow_start.restore();
        self.fastopen.restore();

        if let Some(node) = self.congestion.as_mut() {
            node.restore();
        }

        if let Some(node) = self.qdisc.as_mut() {
            node.restore();
        }
    }

    fn vacate(_bus: &mut Bus) {}
}
