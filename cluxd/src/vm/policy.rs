// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

const DEVICE: &[u8] = b"zram";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Hold,
    Apply,
    Release,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Backing {
    Absent,
    Zram,
    Other,
}

fn is_zram(name: &[u8]) -> bool {
    name.rsplit(|&byte| byte == b'/')
        .next()
        .and_then(|base| base.strip_prefix(DEVICE))
        .is_some_and(|index| !index.is_empty() && index.iter().all(u8::is_ascii_digit))
}

fn backing(swaps: &[u8]) -> Backing {
    let mut zram = false;

    for line in swaps.split(|&byte| byte == b'\n').skip(1) {
        let name = line
            .split(|&byte| byte == b' ' || byte == b'\t')
            .next()
            .unwrap_or_default();

        if name.is_empty() {
            continue;
        }

        if !is_zram(name) {
            return Backing::Other;
        }

        zram = true;
    }

    if zram { Backing::Zram } else { Backing::Absent }
}

pub fn judge(swaps: &[u8], complete: bool) -> Verdict {
    if !complete {
        return Verdict::Hold;
    }

    match backing(swaps) {
        Backing::Absent => Verdict::Hold,
        Backing::Zram => Verdict::Apply,
        Backing::Other => Verdict::Release,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &[u8] = b"Filename\t\t\t\tType\t\tSize\t\tUsed\t\tPriority\n";

    fn table(rows: &[&[u8]]) -> Vec<u8> {
        let mut text = HEADER.to_vec();
        for row in rows {
            text.extend_from_slice(row);
            text.push(b'\n');
        }
        text
    }

    #[test]
    fn android_zram_device_is_tuned() {
        let text =
            table(&[b"/dev/block/zram0                        partition\t4194300\t\t0\t\t-2"]);
        assert_eq!(judge(&text, true), Verdict::Apply);
    }

    #[test]
    fn plain_linux_device_path_is_tuned() {
        let text =
            table(&[b"/dev/zram0                              partition\t524284\t\t0\t\t-2"]);
        assert_eq!(judge(&text, true), Verdict::Apply);
    }

    #[test]
    fn several_zram_devices_are_tuned() {
        let text = table(&[
            b"/dev/block/zram0\tpartition\t1024\t0\t-2",
            b"/dev/block/zram12\tpartition\t1024\t0\t-3",
        ]);
        assert_eq!(judge(&text, true), Verdict::Apply);
    }

    #[test]
    fn disk_backed_swap_is_never_tuned() {
        let text = table(&[b"/data/swapfile\tfile\t1024\t0\t-2"]);
        assert_eq!(judge(&text, true), Verdict::Release);
    }

    #[test]
    fn one_foreign_area_among_zram_releases() {
        let text = table(&[
            b"/dev/block/zram0\tpartition\t1024\t0\t-2",
            b"/data/swapfile\tfile\t1024\t0\t-3",
        ]);
        assert_eq!(judge(&text, true), Verdict::Release);
    }

    #[test]
    fn only_a_device_node_named_zram_counts() {
        for name in [
            b"/data/zram/swapfile".as_slice(),
            b"/data/zramswap.img".as_slice(),
            b"/dev/block/zram".as_slice(),
            b"/dev/block/zram0p1".as_slice(),
            b"/dev/block/myzram0".as_slice(),
        ] {
            let mut row = name.to_vec();
            row.extend_from_slice(b"\tfile\t1024\t0\t-2");
            assert_eq!(
                judge(&table(&[&row]), true),
                Verdict::Release,
                "{}",
                String::from_utf8_lossy(name)
            );
        }
    }

    #[test]
    fn no_swap_leaves_the_node_alone() {
        assert_eq!(judge(HEADER, true), Verdict::Hold);
        assert_eq!(judge(b"", true), Verdict::Hold);
        assert_eq!(judge(b"\n\n", true), Verdict::Hold);
    }

    #[test]
    fn a_possibly_truncated_read_decides_nothing() {
        let text = table(&[b"/data/swapfile\tfile\t1024\t0\t-2"]);
        assert_eq!(judge(&text, false), Verdict::Hold);
    }

    #[test]
    fn the_header_row_is_never_a_swap_area() {
        assert_eq!(judge(b"zram0 header row only", true), Verdict::Hold);
    }

    #[test]
    fn blank_rows_are_ignored() {
        let text = table(&[b"", b"/dev/block/zram0\tpartition\t1024\t0\t-2", b""]);
        assert_eq!(judge(&text, true), Verdict::Apply);
    }
}
