// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use crate::sysfs::lexer;

pub const SLOW_START_OFF: u32 = 0;

const FASTOPEN_CLIENT: u32 = 1;
const GENERIC: [&[u8]; 2] = [b"cubic", b"reno"];
pub const BBR: &[u8] = b"bbr";
const FLAT: &[u8] = b"pfifo_fast";

pub const fn fastopen(origin: u32) -> u32 {
    origin | FASTOPEN_CLIENT
}

pub fn congestion(current: &[u8], available: &[u8]) -> Option<&'static [u8]> {
    let generic = GENERIC.contains(&current);
    let offered = lexer::words(available).any(|name| name == BBR);

    (generic && offered).then_some(BBR)
}

pub fn qdisc(current: &[u8], congestion: &[u8]) -> &'static [&'static [u8]] {
    const PACED: &[&[u8]] = &[b"fq", b"fq_codel"];
    const FAIR: &[&[u8]] = &[b"fq_codel"];
    const NONE: &[&[u8]] = &[];

    if current != FLAT {
        NONE
    } else if congestion == BBR {
        PACED
    } else {
        FAIR
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_open_always_keeps_the_client_side_on() {
        assert_eq!(fastopen(0), 1, "disabled");
        assert_eq!(fastopen(1), 1, "client only");
        assert_eq!(fastopen(2), 3, "server only gains the client");
        assert_eq!(fastopen(3), 3, "both");
        assert_eq!(fastopen(0x405), 0x405, "other bits are kept");
    }

    #[test]
    fn a_generic_default_moves_to_bbr_when_the_kernel_offers_it() {
        assert_eq!(
            congestion(b"cubic", b"reno bbr cubic"),
            Some(BBR),
            "cubic default"
        );
        assert_eq!(
            congestion(b"reno", b"reno bbr\n"),
            Some(BBR),
            "reno default"
        );
    }

    #[test]
    fn nothing_changes_without_bbr_in_the_kernel() {
        assert_eq!(congestion(b"cubic", b"reno cubic"), None, "not built");
        assert_eq!(
            congestion(b"cubic", b"reno bbr2 cubic"),
            None,
            "no substring"
        );
        assert_eq!(congestion(b"cubic", b""), None, "empty list");
    }

    #[test]
    fn a_vendor_choice_is_respected() {
        for current in [&b"bbr"[..], b"bbr2", b"westwood", b"htcp", b""] {
            assert_eq!(congestion(current, b"reno bbr cubic westwood"), None);
        }
    }

    #[test]
    fn bbr_is_paired_with_the_pacing_qdisc() {
        let list = qdisc(FLAT, BBR);
        assert_eq!(list, &[&b"fq"[..], b"fq_codel"], "fq first, codel fallback");
    }

    #[test]
    fn other_algorithms_get_the_codel_qdisc() {
        for current in [&b"cubic"[..], b"reno", b"westwood"] {
            assert_eq!(qdisc(FLAT, current), &[&b"fq_codel"[..]], "{current:?}");
        }
    }

    #[test]
    fn a_qdisc_chosen_by_the_vendor_is_left_alone() {
        for current in [&b"fq_codel"[..], b"fq", b"cake", b"noqueue", b"mq"] {
            assert!(qdisc(current, BBR).is_empty(), "{current:?}");
            assert!(qdisc(current, b"cubic").is_empty(), "{current:?}");
        }
    }

    #[test]
    fn slow_start_after_idle_is_switched_off() {
        assert_eq!(SLOW_START_OFF, 0, "the congestion window survives idle");
    }
}
