// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use crate::abi::{CAP_ABSOLUTE, CAP_CONTACT, CAP_DIRECT, CAP_MULTITOUCH, CAP_POINTER};

pub const HOLD_US: u64 = 2_000_000;
pub const EVENT_LIMIT: u32 = 32;

const fn has(caps: u32, flag: u32) -> bool {
    caps & flag == flag
}

pub const fn touchscreen(caps: u32) -> bool {
    if has(caps, CAP_DIRECT) {
        return true;
    }

    !has(caps, CAP_POINTER) && (has(caps, CAP_MULTITOUCH) || has(caps, CAP_ABSOLUTE | CAP_CONTACT))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_direct_input_device_is_a_touchscreen() {
        assert!(touchscreen(CAP_DIRECT), "marked direct by its driver");
        assert!(
            touchscreen(CAP_DIRECT | CAP_MULTITOUCH | CAP_CONTACT),
            "typical modern panel"
        );
    }

    #[test]
    fn a_panel_without_properties_is_recognised_by_its_axes() {
        assert!(
            touchscreen(CAP_MULTITOUCH | CAP_CONTACT),
            "multi-touch panel"
        );
        assert!(
            touchscreen(CAP_MULTITOUCH),
            "multi-touch without a contact key"
        );
        assert!(
            touchscreen(CAP_ABSOLUTE | CAP_CONTACT),
            "legacy single-touch panel"
        );
    }

    #[test]
    fn a_touchpad_is_not_a_touchscreen() {
        assert!(
            !touchscreen(CAP_POINTER | CAP_MULTITOUCH | CAP_CONTACT),
            "pointer device with multi-touch"
        );
        assert!(
            !touchscreen(CAP_POINTER | CAP_ABSOLUTE | CAP_CONTACT),
            "pointer device with single-touch"
        );
    }

    #[test]
    fn other_input_devices_are_ignored() {
        assert!(!touchscreen(0), "keys and switches");
        assert!(!touchscreen(CAP_ABSOLUTE), "joystick axes without contact");
        assert!(!touchscreen(CAP_CONTACT), "contact key alone");
        assert!(!touchscreen(CAP_POINTER), "mouse");
    }
}
