// Copyright 2026 seclususs
// SPDX-License-Identifier: GPL-3.0-only

#[derive(Clone, Copy)]
enum Whole {
    Start,
    Body,
    Tail,
}

#[derive(Clone, Copy)]
enum Fixed {
    Start,
    Whole,
    Dot,
    Tenth,
    Hundredth,
    Tail,
}

const fn blank(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0)
}

pub fn uint(text: &[u8]) -> Option<u64> {
    let mut state = Whole::Start;
    let mut acc: u64 = 0;

    for &byte in text {
        state = match (state, byte) {
            (Whole::Start | Whole::Body, b'0'..=b'9') => {
                acc = acc
                    .checked_mul(10)?
                    .checked_add(u64::from(byte.wrapping_sub(b'0')))?;

                Whole::Body
            }

            (Whole::Body | Whole::Tail, blank_byte) if blank(blank_byte) => Whole::Tail,
            _ => return None,
        };
    }

    matches!(state, Whole::Body | Whole::Tail).then_some(acc)
}

pub fn int(text: &[u8]) -> Option<i64> {
    match text.split_first() {
        Some((b'-', rest)) => i64::try_from(uint(rest)?).ok()?.checked_neg(),
        _ => i64::try_from(uint(text)?).ok(),
    }
}

pub fn pair(text: &[u8]) -> Option<(u32, u32)> {
    let (head, tail) = text.split_at(text.iter().position(|&byte| byte == b':')?);
    let major = u32::try_from(uint(head)?).ok()?;
    let minor = u32::try_from(uint(tail.get(1..)?)?).ok()?;
    Some((major, minor))
}

pub fn centi(text: &[u8]) -> Option<u32> {
    let mut state = Fixed::Start;
    let mut whole: u32 = 0;
    let mut frac: u32 = 0;

    for &byte in text {
        let digit = u32::from(byte.wrapping_sub(b'0'));

        state = match (state, byte) {
            (Fixed::Start | Fixed::Whole, b'0'..=b'9') => {
                whole = whole.checked_mul(10)?.checked_add(digit)?;
                Fixed::Whole
            }

            (Fixed::Whole, b'.') => Fixed::Dot,
            (Fixed::Dot, b'0'..=b'9') => {
                frac = digit.checked_mul(10)?;
                Fixed::Tenth
            }

            (Fixed::Tenth, b'0'..=b'9') => {
                frac = frac.checked_add(digit)?;
                Fixed::Hundredth
            }

            (Fixed::Hundredth, b'0'..=b'9') => Fixed::Hundredth,
            (Fixed::Whole | Fixed::Tenth | Fixed::Hundredth | Fixed::Tail, blank_byte)
                if blank(blank_byte) =>
            {
                Fixed::Tail
            }

            _ => return None,
        };
    }

    match state {
        Fixed::Start | Fixed::Dot => None,
        Fixed::Whole | Fixed::Tenth | Fixed::Hundredth | Fixed::Tail => {
            whole.checked_mul(100)?.checked_add(frac)
        }
    }
}

pub fn words(line: &[u8]) -> impl Iterator<Item = &[u8]> {
    line.split(u8::is_ascii_whitespace)
        .filter(|word| !word.is_empty())
}

pub fn line<'a>(text: &'a [u8], head: &[u8]) -> Option<&'a [u8]> {
    text.split(|&byte| byte == b'\n')
        .find(|candidate| candidate.starts_with(head))
}

pub fn field<'a>(line: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    words(line).find_map(|word| word.strip_prefix(key)?.strip_prefix(b"="))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PSI: &[u8] = b"some avg10=8.16 avg60=13.48 avg300=4.60 total=16429809\nfull avg10=0.00 avg60=0.00 avg300=0.00 total=0\n";

    #[test]
    fn uint_accepts_trailing_blank_only() {
        assert_eq!(uint(b"8000000\n"), Some(8_000_000));
        assert_eq!(uint(b"42"), Some(42));
        assert_eq!(uint(b""), None);
        assert_eq!(uint(b"\n"), None);
        assert_eq!(uint(b"12 3"), None);
        assert_eq!(uint(b"12a"), None);
        assert_eq!(uint(b"99999999999999999999999"), None);
    }

    #[test]
    fn int_handles_sign() {
        assert_eq!(int(b"-250\n"), Some(-250));
        assert_eq!(int(b"45000"), Some(45_000));
        assert_eq!(int(b"-"), None);
        assert_eq!(int(b"--1"), None);
    }

    #[test]
    fn centi_parses_percent_fixed_point() {
        assert_eq!(centi(b"12.34"), Some(1234));
        assert_eq!(centi(b"0.00"), Some(0));
        assert_eq!(centi(b"5.3"), Some(530));
        assert_eq!(centi(b"7"), Some(700));
        assert_eq!(centi(b"1.239"), Some(123));
        assert_eq!(centi(b"."), None);
        assert_eq!(centi(b"1."), None);
        assert_eq!(centi(b".5"), None);
        assert_eq!(centi(b"1.2.3"), None);
    }

    #[test]
    fn psi_fields_are_found_on_the_some_line() {
        let some = line(PSI, b"some ").expect("some line");
        assert_eq!(field(some, b"avg10").and_then(centi), Some(816));
        assert_eq!(field(some, b"total").and_then(uint), Some(16_429_809));
        assert_eq!(field(some, b"avg30"), None);
        let full = line(PSI, b"full ").expect("full line");
        assert_eq!(field(full, b"total").and_then(uint), Some(0));
        assert_eq!(line(PSI, b"other"), None);
    }

    #[test]
    fn words_skip_runs_of_blanks() {
        let got: Vec<&[u8]> = words(b"  12   7\t9 \n").collect();
        assert_eq!(got, [b"12".as_slice(), b"7".as_slice(), b"9".as_slice()]);
    }

    #[test]
    fn pair_parses_major_minor_lines() {
        assert_eq!(pair(b"8:0\n"), Some((8, 0)));
        assert_eq!(pair(b"259:65537\n"), Some((259, 65537)));
        assert_eq!(pair(b"254:3"), Some((254, 3)));
    }

    #[test]
    fn pair_rejects_malformed_lines() {
        assert_eq!(pair(b""), None);
        assert_eq!(pair(b"8"), None);
        assert_eq!(pair(b":3"), None);
        assert_eq!(pair(b"8:"), None);
        assert_eq!(pair(b"8:x"), None);
        assert_eq!(pair(b"8 : 3"), None);
        assert_eq!(pair(b"99999999999:1"), None);
    }
}
