// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use crate::automata::fixed::ratio;
use crate::sysfs::lexer;

const SIZE_FLOOR_KB: u64 = 8;
const SIZE_SPAN_KB: u64 = 504;
const DEPTH_FLOOR_MILLI: u64 = 250;
const DEPTH_SPAN_MILLI: u64 = 4750;
const PER_MILLE: u64 = 1000;
const SECTORS_PER_KB: u64 = 2;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    pub reads: u64,
    pub merges: u64,
    pub sectors: u64,
    pub read_ms: u64,
    pub writes: u64,
    pub write_ms: u64,
    pub inflight: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Metrics {
    pub sequential: u32,
    pub latency_ms: u32,
}

pub fn parse(text: &[u8]) -> Option<Counters> {
    let mut fields = lexer::words(text).map(lexer::uint);
    let mut take = || fields.next().flatten();
    let reads = take()?;
    let merges = take()?;
    let sectors = take()?;
    let read_ms = take()?;
    let writes = take()?;
    let _write_merges = take()?;
    let _write_sectors = take()?;
    let write_ms = take()?;
    let inflight = take()?;
    Some(Counters {
        reads,
        merges,
        sectors,
        read_ms,
        writes,
        write_ms,
        inflight,
    })
}

pub fn measure(prev: &Counters, now: &Counters) -> Metrics {
    let reads = now.reads.saturating_sub(prev.reads);
    let merges = now.merges.saturating_sub(prev.merges);
    let sectors = now.sectors.saturating_sub(prev.sectors);
    let writes = now.writes.saturating_sub(prev.writes);
    let waited = now
        .read_ms
        .saturating_sub(prev.read_ms)
        .saturating_add(now.write_ms.saturating_sub(prev.write_ms));

    let size_kb = sectors.checked_div(reads).unwrap_or(0) / SECTORS_PER_KB;
    let size = ratio(
        size_kb.saturating_sub(SIZE_FLOOR_KB),
        SIZE_SPAN_KB,
        PER_MILLE,
    )
    .min(1000);

    let joined = ratio(merges, merges.saturating_add(reads), PER_MILLE);
    let depth = ratio(
        now.inflight
            .saturating_mul(PER_MILLE)
            .saturating_sub(DEPTH_FLOOR_MILLI),
        DEPTH_SPAN_MILLI,
        PER_MILLE,
    )
    .min(1000);

    let pattern = u64::from(size.max(joined)).saturating_mul(u64::from(depth)) / PER_MILLE;
    Metrics {
        sequential: u32::try_from(pattern).unwrap_or(1000),
        latency_ms: ratio(waited, reads.saturating_add(writes), 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAT: &[u8] = b"  120145     3021  4823012   98211    55312     9120  1204112   40211        3   130021   138422        0        0        0        0\n";

    #[test]
    fn parses_the_leading_counters() {
        let counters = parse(STAT).expect("counters");
        assert_eq!(counters.reads, 120_145);
        assert_eq!(counters.sectors, 4_823_012);
        assert_eq!(counters.write_ms, 40_211);
        assert_eq!(counters.inflight, 3);
        assert_eq!(parse(b"1 2 3"), None);
        assert_eq!(parse(b"a b c d e f g h i"), None);
    }

    #[test]
    fn large_merged_deep_reads_look_sequential() {
        let prev = Counters::default();
        let now = Counters {
            reads: 100,
            merges: 400,
            sectors: 100 * 2048,
            inflight: 5,
            ..prev
        };
        let metrics = measure(&prev, &now);
        assert_eq!(metrics.sequential, 1000);
    }

    #[test]
    fn small_unmerged_shallow_reads_look_random() {
        let prev = Counters::default();
        let now = Counters {
            reads: 400,
            merges: 4,
            sectors: 400 * 8,
            inflight: 1,
            ..prev
        };
        let metrics = measure(&prev, &now);
        assert!(metrics.sequential < 60, "{}", metrics.sequential);
    }

    #[test]
    fn latency_is_average_wait_per_request() {
        let prev = Counters::default();
        let now = Counters {
            reads: 10,
            writes: 10,
            read_ms: 300,
            write_ms: 100,
            ..prev
        };
        assert_eq!(measure(&prev, &now).latency_ms, 20);
        assert_eq!(measure(&prev, &prev).latency_ms, 0);
    }

    #[test]
    fn counter_resets_do_not_underflow() {
        let prev = Counters {
            reads: 50,
            sectors: 900,
            ..Counters::default()
        };
        let metrics = measure(&prev, &Counters::default());
        assert_eq!(metrics, Metrics::default());
    }
}
