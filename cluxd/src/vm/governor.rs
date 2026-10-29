// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use super::policy::{Verdict, judge};
use crate::abi::{self, Fault, Fd, Outcome};
use crate::automata::cadence::Cadence;
use crate::event::slot::{Cause, Unit};
use crate::psi::Bus;
use crate::sysfs::knob::Knob;

const SWAPS: &CStr = c"/proc/swaps";
const PAGE_CLUSTER: &CStr = c"/proc/sys/vm/page-cluster";
const WATERMARK_BOOST: &CStr = c"/proc/sys/vm/watermark_boost_factor";
const COMPACTION_PROACTIVENESS: &CStr = c"/proc/sys/vm/compaction_proactiveness";
const SWAPS_CAP: usize = 512;
const NO_READAHEAD: u32 = 0;
const NO_BOOST: u32 = 0;
const NO_COMPACTION: u32 = 0;
const PACE_MS: [u32; 3] = [30_000, 120_000, 600_000];
const PACE_PATIENCE: u32 = 2;

#[derive(Debug)]
pub struct Governor {
    cluster: Knob,
    boost: Knob,
    compact: Knob,
    cadence: Cadence<3>,
    verdict: Verdict,
}

fn is_tunable(origin: Option<u32>, target: u32) -> bool {
    origin.is_some_and(|value| value != target)
}

impl Governor {
    fn enforce_cluster(&mut self) {
        if self
            .cluster
            .level()
            .is_some_and(|level| level != NO_READAHEAD)
        {
            let _ = self.cluster.force(NO_READAHEAD);
        }
    }

    fn enforce_boost(&mut self) {
        if self.boost.level().is_some_and(|level| level != NO_BOOST) {
            let _ = self.boost.force(NO_BOOST);
        }
    }

    fn enforce_compact(&mut self) {
        if self
            .compact
            .level()
            .is_some_and(|level| level != NO_COMPACTION)
        {
            let _ = self.compact.force(NO_COMPACTION);
        }
    }
}

impl Unit for Governor {
    const STARTED: &'static CStr = c"vm governor online";
    const FAILED: &'static CStr = c"vm governor stopped, fault";

    fn spawn(_now_us: u64) -> Outcome<Self> {
        let cluster = Knob::open(PAGE_CLUSTER);
        let boost = Knob::open(WATERMARK_BOOST);
        let compact = Knob::open(COMPACTION_PROACTIVENESS);

        let cluster_tunable = is_tunable(cluster.origin(), NO_READAHEAD);
        let boost_tunable = is_tunable(boost.origin(), NO_BOOST);
        let compact_tunable = is_tunable(compact.origin(), NO_COMPACTION);

        if !cluster_tunable && !boost_tunable && !compact_tunable {
            return Err(Fault::Absent);
        }

        let mut cadence = Cadence::new(PACE_MS, PACE_PATIENCE);
        cadence.boost();

        Ok(Self {
            cluster,
            boost,
            compact,
            cadence,
            verdict: Verdict::Hold,
        })
    }

    fn watch(&self) -> Option<&Fd> {
        None
    }

    fn tick(&mut self, now_us: u64, _bus: &mut Bus, _cause: Cause) -> Outcome<u64> {
        let mut buf = [0_u8; SWAPS_CAP];
        let got = abi::slurp(SWAPS, &mut buf)?;
        let text = buf.get(..got).ok_or(Fault::Parse)?;
        let verdict = judge(text, got < SWAPS_CAP);

        match verdict {
            Verdict::Apply => {
                self.enforce_cluster();
                self.enforce_boost();
                self.enforce_compact();
            }
            Verdict::Release => {
                self.cluster.restore();
                self.enforce_boost();
                self.enforce_compact();
            }
            Verdict::Hold => {
                self.enforce_boost();
                self.enforce_compact();
            }
        }

        if verdict != self.verdict {
            self.verdict = verdict;
            self.cadence.boost();
        }

        Ok(now_us.saturating_add(self.cadence.next_us(usize::MAX)))
    }

    fn retire(&mut self) {
        self.cluster.restore();
        self.boost.restore();
        self.compact.restore();
    }

    fn vacate(_bus: &mut Bus) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tunable_when_origin_differs_from_target() {
        assert!(is_tunable(Some(3), NO_READAHEAD));
        assert!(is_tunable(Some(15000), NO_BOOST));
        assert!(is_tunable(Some(20), NO_COMPACTION));
        assert!(!is_tunable(Some(0), NO_READAHEAD));
        assert!(!is_tunable(Some(0), NO_BOOST));
        assert!(!is_tunable(Some(0), NO_COMPACTION));
        assert!(!is_tunable(None, NO_READAHEAD));
        assert!(!is_tunable(None, NO_BOOST));
        assert!(!is_tunable(None, NO_COMPACTION));
    }
}
