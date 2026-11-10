// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

pub const INTERVAL_US: u64 = 5_000_000;

const CURRENT_FLOOR_UA: u64 = 100_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Charging,
    Discharging,
    Held,
    Unknown,
}

pub const fn status(text: &[u8]) -> Status {
    match text.trim_ascii() {
        b"Charging" => Status::Charging,
        b"Discharging" => Status::Discharging,
        b"Not charging" | b"Full" => Status::Held,
        _ => Status::Unknown,
    }
}

pub const fn plugged(status: Status, current_ua: Option<i64>, held: bool) -> bool {
    match status {
        Status::Charging | Status::Held => true,
        Status::Discharging => false,
        Status::Unknown => match current_ua {
            Some(ua) if ua.unsigned_abs() >= CURRENT_FLOOR_UA => ua > 0,
            _ => held,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_follows_the_power_supply_abi() {
        assert_eq!(status(b"Charging\n"), Status::Charging, "charging");
        assert_eq!(status(b"Discharging\n"), Status::Discharging, "on battery");
        assert_eq!(status(b"Not charging\n"), Status::Held, "charge inhibited");
        assert_eq!(status(b"Full\n"), Status::Held, "full");
        assert_eq!(status(b"Unknown\n"), Status::Unknown, "unknown");
    }

    #[test]
    fn unreadable_status_is_unknown() {
        assert_eq!(status(b""), Status::Unknown, "empty read");
        assert_eq!(status(b"\n"), Status::Unknown, "blank line");
        assert_eq!(status(b"charging"), Status::Unknown, "wrong case");
        assert_eq!(status(b"Charging now"), Status::Unknown, "trailing words");
    }

    #[test]
    fn external_power_means_plugged() {
        for status in [Status::Charging, Status::Held] {
            assert!(plugged(status, None, false), "{status:?} is on the supply");
            assert!(
                plugged(status, Some(-900_000), false),
                "{status:?} ignores the current sign"
            );
        }
    }

    #[test]
    fn discharging_means_on_battery() {
        assert!(!plugged(Status::Discharging, None, true), "no current");
        assert!(
            !plugged(Status::Discharging, Some(900_000), true),
            "status wins over the current"
        );
    }

    #[test]
    fn an_unknown_status_falls_back_to_a_clear_current() {
        assert!(
            plugged(Status::Unknown, Some(1_200_000), false),
            "positive current charges"
        );
        assert!(
            !plugged(Status::Unknown, Some(-450_000), true),
            "negative current discharges"
        );
    }

    #[test]
    fn an_unknown_status_keeps_the_last_state_when_the_current_is_unclear() {
        assert!(plugged(Status::Unknown, None, true), "no current kept");
        assert!(!plugged(Status::Unknown, None, false), "no current kept");
        assert!(
            plugged(Status::Unknown, Some(40_000), true),
            "noise below the floor"
        );
        assert!(
            !plugged(Status::Unknown, Some(-40_000), false),
            "noise below the floor"
        );
    }
}
