// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use crate::abi::{self, Fault, Fd, Mode, Outcome};
use crate::automata::envelope::Envelope;
use crate::automata::fixed::ratio;
use crate::sysfs::lexer;

const READ_CAP: usize = 256;
const MIN_SPAN_US: u64 = 10_000;
const FULL_SCALE: u32 = 1000;
const ATTACK: u32 = 2;
const RELEASE: u32 = 8;
const SNAP: u32 = 250;

#[derive(Clone, Copy, Debug)]
pub struct Trigger {
    pub threshold_us: u32,
    pub window_us: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sample {
    pub level: u32,
    pub slope: i32,
}

#[derive(Debug)]
pub struct Gauge {
    watch: Option<Fd>,
    reader: Fd,
    tracker: Tracker,
}

#[derive(Debug)]
struct Tracker {
    envelope: Envelope,
    total: u64,
    stamp: u64,
    primed: bool,
}

impl Gauge {
    pub fn open(path: &CStr, trigger: Trigger) -> Outcome<Self> {
        let reader = abi::open(path, Mode::Read)?;
        let watch = abi::arm(path, trigger.threshold_us, trigger.window_us).ok();
        Ok(Self {
            watch,
            reader,
            tracker: Tracker::new(),
        })
    }

    pub const fn watch(&self) -> Option<&Fd> {
        self.watch.as_ref()
    }

    pub fn sample(&mut self, now_us: u64, stale_us: u64) -> Outcome<Sample> {
        let mut buf = [0_u8; READ_CAP];
        let got = self.reader.read(&mut buf)?;
        let text = buf.get(..got).ok_or(Fault::Parse)?;

        self.tracker.observe(text, now_us, stale_us)
    }
}

impl Tracker {
    fn new() -> Self {
        Self {
            envelope: Envelope::new(ATTACK, RELEASE, SNAP),
            total: 0,
            stamp: 0,
            primed: false,
        }
    }

    fn observe(&mut self, text: &[u8], now_us: u64, stale_us: u64) -> Outcome<Sample> {
        let some = lexer::line(text, b"some ").ok_or(Fault::Parse)?;
        let total = lexer::field(some, b"total")
            .and_then(lexer::uint)
            .ok_or(Fault::Parse)?;

        let span = now_us.saturating_sub(self.stamp);

        if !self.primed {
            let seed = lexer::field(some, b"avg10")
                .and_then(lexer::centi)
                .unwrap_or(0);

            self.envelope.reset((seed / 10).min(FULL_SCALE));
        } else if span >= stale_us {
            self.envelope.reset(self.measure(total, span));
        } else if span >= MIN_SPAN_US {
            let _ = self.envelope.feed(self.measure(total, span));
        } else {
            return Ok(self.current());
        }

        self.primed = true;
        self.total = total;
        self.stamp = now_us;
        Ok(self.current())
    }

    fn measure(&self, total: u64, span: u64) -> u32 {
        ratio(
            total.saturating_sub(self.total),
            span,
            u64::from(FULL_SCALE),
        )
        .min(FULL_SCALE)
    }

    const fn current(&self) -> Sample {
        Sample {
            level: self.envelope.level(),
            slope: self.envelope.slope(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND_US: u64 = 1_000_000;
    const STALE_US: u64 = 21 * SECOND_US;
    const BASE_TOTAL: u64 = 1_000_000;

    fn pressure(avg10: &str, total: u64) -> String {
        format!(
            "some avg10={avg10} avg60=0.00 avg300=0.00 total={total}\n\
             full avg10=0.00 avg60=0.00 avg300=0.00 total=0\n"
        )
    }

    fn primed() -> Tracker {
        let mut tracker = Tracker::new();
        let text = pressure("8.16", BASE_TOTAL);
        let _ = tracker
            .observe(text.as_bytes(), 10 * SECOND_US, STALE_US)
            .expect("prime");
        tracker
    }

    fn level_after(tracker: &mut Tracker, total: u64, at_us: u64) -> Sample {
        let text = pressure("8.16", total);
        tracker
            .observe(text.as_bytes(), at_us, STALE_US)
            .expect("sample")
    }

    #[test]
    fn the_first_sample_is_seeded_from_avg10() {
        let mut tracker = Tracker::new();
        let text = pressure("8.16", BASE_TOTAL);
        let sample = tracker.observe(text.as_bytes(), 10 * SECOND_US, STALE_US);
        assert_eq!(
            sample,
            Ok(Sample {
                level: 81,
                slope: 0
            })
        );
    }

    #[test]
    fn an_unreadable_avg10_seeds_zero() {
        let mut tracker = Tracker::new();
        let text = pressure("n/a", BASE_TOTAL);
        let sample = tracker.observe(text.as_bytes(), 10 * SECOND_US, STALE_US);
        assert_eq!(sample, Ok(Sample { level: 0, slope: 0 }));
    }

    #[test]
    fn later_samples_follow_the_stall_share() {
        let mut tracker = primed();
        let sample = level_after(&mut tracker, BASE_TOTAL + 100_000, 11 * SECOND_US);
        assert_eq!(sample.level, 91);
        assert!(sample.slope > 0);
    }

    #[test]
    fn a_sample_inside_the_minimum_span_changes_nothing() {
        let mut tracker = primed();
        let first = level_after(&mut tracker, BASE_TOTAL + 100_000, 11 * SECOND_US);
        let early = level_after(&mut tracker, BASE_TOTAL + 100_000, 11 * SECOND_US + 5_000);
        assert_eq!(early, first);

        let sample = level_after(&mut tracker, BASE_TOTAL + 700_000, 12 * SECOND_US);
        assert_eq!(sample.level, 600);
    }

    #[test]
    fn a_stale_gap_resets_to_the_measured_average() {
        let mut tracker = primed();
        let sample = level_after(&mut tracker, BASE_TOTAL + 3_000_000, 40 * SECOND_US);
        assert_eq!(
            sample,
            Sample {
                level: 100,
                slope: 0
            }
        );
    }

    #[test]
    fn a_counter_that_goes_backwards_reads_as_idle() {
        let mut tracker = primed();
        let sample = level_after(&mut tracker, 500, 11 * SECOND_US);
        assert_eq!(sample.level, 70);
    }

    #[test]
    fn a_stall_longer_than_the_span_is_clamped() {
        let mut tracker = primed();
        let sample = level_after(&mut tracker, BASE_TOTAL + 9_000_000, 11 * SECOND_US);
        assert_eq!(sample.level, FULL_SCALE);
    }

    #[test]
    fn malformed_text_is_a_parse_fault() {
        let mut tracker = primed();
        let garbage = tracker.observe(b"garbage\n", 11 * SECOND_US, STALE_US);
        assert_eq!(garbage, Err(Fault::Parse));

        let no_total = tracker.observe(b"some avg10=1.00\n", 11 * SECOND_US, STALE_US);
        assert_eq!(no_total, Err(Fault::Parse));
    }
}
