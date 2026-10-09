// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

use core::ffi::CStr;

use super::lexer;
use super::path::Path;
use crate::abi;

const DISKS: [&[u8]; 4] = [b"nvme0n1", b"sda", b"sdb", b"mmcblk0"];
const SLAVES_CAP: usize = 256;
const STACK_LIMIT: usize = 4;
const DATA: &CStr = c"/data";

#[derive(Clone, Copy, Debug)]
pub struct Disk {
    top: Path<64>,
    low: Path<64>,
}

trait Tree {
    fn has(&self, path: &Path<64>) -> bool;
    fn lower(&self, disk: &Path<64>) -> Option<(u32, u32)>;
}

struct Live;

impl Tree for Live {
    fn has(&self, path: &Path<64>) -> bool {
        path.cstr()
            .is_some_and(|text| abi::open(text, abi::Mode::Read).is_ok())
    }

    fn lower(&self, disk: &Path<64>) -> Option<(u32, u32)> {
        let slaves = disk.push(b"/slaves")?;
        let mut names = [0_u8; SLAVES_CAP];
        let used = abi::scan(slaves.cstr()?, &mut names).ok()?;

        let name = names
            .get(..used)?
            .split(|&byte| byte == 0)
            .next()
            .filter(|name| !name.is_empty())?;

        let node = Path::<64>::new()
            .push(b"/sys/class/block/")?
            .push(name)?
            .push(b"/dev")?;

        let mut buf = [0_u8; 24];
        let got = abi::slurp(node.cstr()?, &mut buf).ok()?;
        lexer::pair(buf.get(..got)?)
    }
}

fn origin(major: u32, minor: u32) -> Option<Path<64>> {
    Path::<64>::new()
        .push(b"/sys/dev/block/")?
        .push_uint(u64::from(major))?
        .push(b":")?
        .push_uint(u64::from(minor))
}

fn owner<T: Tree>(tree: &T, base: Path<64>) -> Option<Path<64>> {
    if tree.has(&base.push(b"/partition")?) {
        base.push(b"/..")
    } else {
        Some(base)
    }
}

fn descend<T: Tree>(tree: &T, major: u32, minor: u32) -> Option<(Path<64>, Path<64>)> {
    let top = owner(tree, origin(major, minor)?)?;
    let mut low = top;

    for _ in 0..STACK_LIMIT {
        let Some((major, minor)) = tree.lower(&low) else {
            return Some((top, low));
        };

        low = owner(tree, origin(major, minor)?)?;
    }

    None
}

impl Disk {
    pub fn find() -> Option<Self> {
        Self::of(DATA).or_else(|| Self::named(&Live))
    }

    pub fn of(mount: &CStr) -> Option<Self> {
        let (major, minor) = abi::devno(mount).ok()?;
        Self::resolve(&Live, major, minor)
    }

    fn resolve<T: Tree>(tree: &T, major: u32, minor: u32) -> Option<Self> {
        let (top, low) = descend(tree, major, minor)?;
        let disk = Self { top, low };
        let usable = tree.has(&disk.node(b"/queue/nr_requests")?)
            && tree.has(&disk.lead(b"/queue/read_ahead_kb")?);

        usable.then_some(disk)
    }

    fn named<T: Tree>(tree: &T) -> Option<Self> {
        DISKS.iter().find_map(|&dev| {
            let base = Path::<64>::new().push(b"/sys/block/")?.push(dev)?;
            let disk = Self {
                top: base,
                low: base,
            };

            tree.has(&disk.node(b"/queue/nr_requests")?).then_some(disk)
        })
    }

    pub fn node(self, tail: &[u8]) -> Option<Path<64>> {
        self.low.push(tail)
    }

    pub fn lead(self, tail: &[u8]) -> Option<Path<64>> {
        self.top.push(tail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Link = (&'static str, (u32, u32));

    struct Fake {
        files: &'static [&'static str],
        stack: &'static [Link],
    }

    impl Tree for Fake {
        fn has(&self, path: &Path<64>) -> bool {
            path.cstr().is_some_and(|text| {
                self.files
                    .iter()
                    .any(|file| text.to_bytes() == file.as_bytes())
            })
        }

        fn lower(&self, disk: &Path<64>) -> Option<(u32, u32)> {
            let text = disk.cstr()?;
            self.stack
                .iter()
                .find(|(base, _)| text.to_bytes() == base.as_bytes())
                .map(|&(_, device)| device)
        }
    }

    fn shown(path: Path<64>) -> String {
        String::from_utf8_lossy(path.cstr().unwrap().to_bytes()).into_owned()
    }

    #[test]
    fn whole_disk_is_its_own_owner() {
        let tree = Fake {
            files: &[
                "/sys/dev/block/8:0/queue/nr_requests",
                "/sys/dev/block/8:0/queue/read_ahead_kb",
            ],
            stack: &[],
        };
        let disk = Disk::resolve(&tree, 8, 0).unwrap();
        assert_eq!(shown(disk.top), "/sys/dev/block/8:0");
        assert_eq!(shown(disk.low), "/sys/dev/block/8:0");
    }

    #[test]
    fn partition_resolves_to_its_parent_disk() {
        let tree = Fake {
            files: &[
                "/sys/dev/block/8:26/partition",
                "/sys/dev/block/8:26/../queue/nr_requests",
                "/sys/dev/block/8:26/../queue/read_ahead_kb",
            ],
            stack: &[],
        };
        let disk = Disk::resolve(&tree, 8, 26).unwrap();
        assert_eq!(
            shown(disk.node(b"/stat").unwrap()),
            "/sys/dev/block/8:26/../stat"
        );
    }

    #[test]
    fn device_mapper_reads_ahead_on_top_but_queues_on_the_bottom() {
        let tree = Fake {
            files: &[
                "/sys/dev/block/254:5/queue/read_ahead_kb",
                "/sys/dev/block/8:26/partition",
                "/sys/dev/block/8:26/../queue/nr_requests",
                "/sys/dev/block/8:26/../queue/read_ahead_kb",
            ],
            stack: &[("/sys/dev/block/254:5", (8, 26))],
        };
        let disk = Disk::resolve(&tree, 254, 5).unwrap();
        assert_eq!(
            shown(disk.lead(b"/queue/read_ahead_kb").unwrap()),
            "/sys/dev/block/254:5/queue/read_ahead_kb"
        );
        assert_eq!(
            shown(disk.node(b"/queue/nr_requests").unwrap()),
            "/sys/dev/block/8:26/../queue/nr_requests"
        );
    }

    #[test]
    fn stacking_cycles_and_depth_overflow_are_rejected() {
        let tree = Fake {
            files: &[
                "/sys/dev/block/254:1/queue/nr_requests",
                "/sys/dev/block/254:1/queue/read_ahead_kb",
            ],
            stack: &[
                ("/sys/dev/block/254:1", (254, 2)),
                ("/sys/dev/block/254:2", (254, 1)),
            ],
        };
        assert!(Disk::resolve(&tree, 254, 1).is_none());
    }

    #[test]
    fn unreadable_queue_is_not_accepted() {
        let tree = Fake {
            files: &["/sys/dev/block/8:0/queue/read_ahead_kb"],
            stack: &[],
        };
        assert!(Disk::resolve(&tree, 8, 0).is_none());
    }

    #[test]
    fn named_fallback_keeps_the_legacy_order() {
        let tree = Fake {
            files: &[
                "/sys/block/mmcblk0/queue/nr_requests",
                "/sys/block/sdb/queue/nr_requests",
            ],
            stack: &[],
        };
        let disk = Disk::named(&tree).unwrap();
        assert_eq!(shown(disk.low), "/sys/block/sdb");
    }
}
