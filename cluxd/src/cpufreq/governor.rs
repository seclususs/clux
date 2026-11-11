// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use super::policy::{Input, Origin, Policy, plan};
use crate::abi::{self, Fault, Fd, Outcome};
use crate::event::slot::{Cause, Unit};
use crate::psi::Bus;
use crate::sysfs::knob::Knob;
use crate::sysfs::path::Path;

const CPUFREQ: &CStr = c"/sys/devices/system/cpu/cpufreq";
const LIST_CAP: usize = 512;
const CLUSTERS: usize = 8;
const RESCAN_US: u64 = 30_000_000;

type Clusters = [Option<Cluster>; CLUSTERS];

#[derive(Debug)]
struct Cluster {
    up: Knob,
    down: Option<Knob>,
}

#[derive(Debug)]
pub struct Governor {
    clusters: Clusters,
    policy: Policy,
}

fn is_policy(name: &[u8]) -> bool {
    name.strip_prefix(b"policy")
        .is_some_and(|rest| !rest.is_empty() && rest.iter().all(u8::is_ascii_digit))
}

fn tunable(policy: &[u8], leaf: &[u8]) -> Option<Knob> {
    let path = Path::<96>::new()
        .push(b"/sys/devices/system/cpu/cpufreq/")?
        .push(policy)?
        .push(b"/schedutil/")?
        .push(leaf)?;

    let knob = Knob::open(path.cstr()?);
    (knob.present() && knob.origin().is_some()).then_some(knob)
}

fn cluster(policy: &[u8]) -> Option<Cluster> {
    if let Some(up) = tunable(policy, b"up_rate_limit_us")
        && let Some(down) = tunable(policy, b"down_rate_limit_us")
    {
        return Some(Cluster {
            up,
            down: Some(down),
        });
    }

    tunable(policy, b"rate_limit_us").map(|up| Cluster { up, down: None })
}

fn discover() -> Clusters {
    let mut clusters = Clusters::default();
    let mut names = [0_u8; LIST_CAP];

    let Some(listed) = abi::scan(CPUFREQ, &mut names)
        .ok()
        .and_then(|used| names.get(..used))
    else {
        return clusters;
    };

    let found = listed
        .split(|&byte| byte == 0)
        .filter(|name| is_policy(name))
        .filter_map(cluster);

    for (slot, cluster) in clusters.iter_mut().zip(found) {
        *slot = Some(cluster);
    }

    clusters
}

fn missing(clusters: &Clusters) -> bool {
    clusters.iter().all(Option::is_none)
}

impl Cluster {
    fn alive(&self) -> bool {
        self.up.level().is_some()
    }

    fn apply(&mut self, regime: u32) {
        let origin = Origin {
            up: self.up.origin().unwrap_or(0),
            down: self.down.as_ref().and_then(Knob::origin).unwrap_or(0),
        };

        let plan = plan(origin, regime, self.down.is_some());

        self.up.set(plan.up);
        if let Some(down) = self.down.as_mut() {
            down.set(plan.down);
        }
    }

    fn restore(&mut self) {
        self.up.restore();
        if let Some(down) = self.down.as_mut() {
            down.restore();
        }
    }
}

impl Unit for Governor {
    const STARTED: &'static CStr = c"cpufreq governor online";
    const FAILED: &'static CStr = c"cpufreq governor stopped, fault";

    fn spawn(_now_us: u64) -> Outcome<Self> {
        let clusters = discover();

        if missing(&clusters) {
            return Err(Fault::Absent);
        }

        Ok(Self {
            clusters,
            policy: Policy::new(),
        })
    }

    fn watch(&self) -> Option<&Fd> {
        None
    }

    fn tick(&mut self, now_us: u64, bus: &mut Bus, _cause: Cause) -> Outcome<u64> {
        if !self.clusters.iter().flatten().any(Cluster::alive) {
            self.clusters = discover();
        }

        if missing(&self.clusters) {
            return Ok(now_us.saturating_add(RESCAN_US));
        }

        let decision = self.policy.step(
            Input {
                cpu: bus.cpu,
                screen: bus.screen,
                engaged: bus.touch_until > now_us,
            },
            now_us,
        );

        if let Some(regime) = decision.regime {
            self.clusters
                .iter_mut()
                .flatten()
                .for_each(|cluster| cluster.apply(regime));
        }

        Ok(now_us.saturating_add(decision.next_us))
    }

    fn retire(&mut self) {
        self.clusters
            .iter_mut()
            .flatten()
            .for_each(Cluster::restore);
    }

    fn vacate(_bus: &mut Bus) {}
}
