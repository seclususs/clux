// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use super::lexer;
use super::path::Path;
use crate::abi;

const DISKS: [&[u8]; 4] = [b"nvme0n1", b"sda", b"sdb", b"mmcblk0"];
const ZONES: &CStr = c"/sys/class/thermal";
const PRIORITY: [&[u8]; 31] = [
    b"cpu-1-0-usr",
    b"cpu-1-1-usr",
    b"cpu-1-2-usr",
    b"cpu-1-3-usr",
    b"cpu-0-0-usr",
    b"cpu-0-1-usr",
    b"big-core",
    b"mid-core",
    b"little-core",
    b"cpu0_thermal",
    b"cpu1_thermal",
    b"mtktscpu",
    b"mtk_ts_cpu",
    b"mtkts_cpu",
    b"thermal-cpuss-0",
    b"thermal-cpuss-1",
    b"exynos_thermal",
    b"exynos_dev_thermal",
    b"hisi_thermal",
    b"mtktsAP",
    b"mtk_ts_ap",
    b"ap_cdev",
    b"ap_thermal",
    b"soc_thermal",
    b"soc-thermal",
    b"cpu_thermal",
    b"cpu-thermal",
    b"cpu",
    b"tsens_tz_sensor10",
    b"tsens_tz_sensor5",
    b"tsens_tz_sensor0",
];
const LIKELY: [&[u8]; 3] = [b"cpu", b"soc", b"cluster"];
const BANNED: [&[u8]; 20] = [
    b"battery",
    b"bms",
    b"bat",
    b"charger",
    b"usb",
    b"pa_therm",
    b"pa-therm",
    b"modem",
    b"wifi",
    b"wlan",
    b"gpu",
    b"camera",
    b"flash",
    b"led",
    b"pmic",
    b"buck",
    b"ldo",
    b"xo_therm",
    b"quiet",
    b"backlight",
];
const ZONE_PREFIX: &[u8] = b"thermal_zone";
const LIST_CAP: usize = 4096;
const KIND_CAP: usize = 48;
const CORE_LIMIT: u32 = 64;

#[derive(Clone, Copy, Debug)]
pub struct Disk {
    dev: &'static [u8],
}

impl Disk {
    pub fn find() -> Option<Self> {
        DISKS.iter().find_map(|&dev| {
            let probe = Self { dev };
            let path = probe.node(b"/queue/nr_requests")?;
            abi::open(path.cstr()?, abi::Mode::Read).ok().map(|_| probe)
        })
    }

    pub fn node(self, tail: &[u8]) -> Option<Path<64>> {
        Path::new().push(b"/sys/block/")?.push(self.dev)?.push(tail)
    }
}

pub fn read_uint(path: &CStr) -> Option<u64> {
    let mut buf = [0_u8; 24];
    let got = abi::slurp(path, &mut buf).ok()?;
    lexer::uint(buf.get(..got)?)
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

fn rank(kind: &[u8]) -> Option<usize> {
    if let Some(index) = PRIORITY
        .iter()
        .position(|name| name.eq_ignore_ascii_case(kind))
    {
        return Some(index);
    }

    let likely = LIKELY.iter().any(|name| contains(kind, name));
    let banned = BANNED.iter().any(|name| contains(kind, name));
    (likely && !banned).then_some(PRIORITY.len())
}

fn zone_kind(zone: &[u8], out: &mut [u8; KIND_CAP]) -> Option<usize> {
    let path = Path::<64>::new()
        .push(b"/sys/class/thermal/")?
        .push(zone)?
        .push(b"/type")?;

    let got = abi::slurp(path.cstr()?, out).ok()?;
    let text = out.get(..got)?;
    Some(
        text.iter()
            .position(|&byte| byte == b'\n')
            .unwrap_or(text.len()),
    )
}

pub fn zone() -> Option<Path<64>> {
    let mut names = [0_u8; LIST_CAP];
    let used = abi::scan(ZONES, &mut names).ok()?;
    let mut best: Option<(usize, Path<64>)> = None;

    for zone in names.get(..used)?.split(|&byte| byte == 0) {
        if !zone.starts_with(ZONE_PREFIX) {
            continue;
        }

        let mut kind = [0_u8; KIND_CAP];

        let Some(len) = zone_kind(zone, &mut kind) else {
            continue;
        };

        let Some(score) = kind.get(..len).and_then(rank) else {
            continue;
        };

        if best.as_ref().is_some_and(|&(held, _)| held <= score) {
            continue;
        }

        let Some(path) = Path::new()
            .push(b"/sys/class/thermal/")
            .and_then(|path| path.push(zone))
            .and_then(|path| path.push(b"/temp"))
        else {
            continue;
        };

        best = Some((score, path));
    }

    best.map(|(_, path)| path)
}

fn weight(cpu: u32) -> Option<u64> {
    let base = Path::<64>::new()
        .push(b"/sys/devices/system/cpu/cpu")?
        .push_uint(u64::from(cpu))?;
    [
        b"/cpu_capacity".as_slice(),
        b"/cpufreq/cpuinfo_max_freq".as_slice(),
    ]
    .iter()
    .find_map(|tail| read_uint(base.push(tail)?.cstr()?))
}

pub fn cores() -> Option<u64> {
    let count = abi::cores().ok()?.min(CORE_LIMIT);
    let mut lowest = u64::MAX;
    let mut mask = 0_u64;

    for cpu in 0..count {
        let (Some(value), Some(bit)) = (weight(cpu), 1_u64.checked_shl(cpu)) else {
            continue;
        };

        if value < lowest {
            lowest = value;
            mask = bit;
        } else if value == lowest {
            mask |= bit;
        }
    }

    (mask != 0).then_some(mask)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_beats_heuristics() {
        assert_eq!(rank(b"cpu-1-0-usr"), Some(0));
        assert_eq!(rank(b"CPU-THERMAL"), Some(26));
        assert_eq!(rank(b"soc_temp0"), Some(PRIORITY.len()));
    }

    #[test]
    fn banned_and_unrelated_zones_are_skipped() {
        assert_eq!(rank(b"battery"), None);
        assert_eq!(rank(b"cpu_gpu_mix"), None);
        assert_eq!(rank(b"quiet-therm"), None);
        assert_eq!(rank(b"sdm-modem"), None);
    }

    #[test]
    fn contains_is_case_blind_and_total() {
        assert!(contains(b"Xo_THERM", b"xo_therm"));
        assert!(!contains(b"ab", b"abc"));
    }
}
