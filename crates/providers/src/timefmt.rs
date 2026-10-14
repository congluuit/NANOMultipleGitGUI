//! Minimal timestamp parsing for provider APIs (no date-time dependency needed).
//!
//! Accepted forms (all → Unix seconds, UTC):
//! - `2024-05-01` (GitLab token `expires_at`; midnight UTC)
//! - `2024-05-01T10:20:30Z`, `2024-05-01T10:20:30.123+07:00` (RFC 3339 / ISO 8601)
//! - `2024-05-01 10:20:30 UTC`, `2024-05-01 10:20:30 +0700` (GitHub
//!   `github-authentication-token-expiration` header)

use std::time::{SystemTime, UNIX_EPOCH};

/// Current time as Unix seconds.
pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Parses the forms listed in the module docs. Returns `None` for anything else.
pub fn parse_timestamp(input: &str) -> Option<i64> {
    let s = input.trim();
    let date = s.get(..10)?;
    let days = parse_date(date)?;
    let rest = &s[10..];
    if rest.is_empty() {
        return Some(days * 86_400);
    }
    let rest = rest.strip_prefix(['T', 't', ' '])?;
    let hms = rest.get(..8)?;
    let mut parts = hms.split(':');
    let h: i64 = num(parts.next()?, 2)?;
    let m: i64 = num(parts.next()?, 2)?;
    let sec: i64 = num(parts.next()?, 2)?;
    if h > 23 || m > 59 || sec > 60 {
        return None;
    }
    let mut tail = &rest[8..];
    if let Some(frac) = tail.strip_prefix('.') {
        let digits = frac.chars().take_while(char::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        tail = &frac[digits..];
    }
    let offset = parse_offset(tail.trim())?;
    Some(days * 86_400 + h * 3600 + m * 60 + sec - offset)
}

fn num(s: &str, len: usize) -> Option<i64> {
    (s.len() == len && s.bytes().all(|b| b.is_ascii_digit()))
        .then(|| s.parse().ok())
        .flatten()
}

/// Offset in seconds east of UTC.
fn parse_offset(s: &str) -> Option<i64> {
    match s {
        "" | "Z" | "z" | "UTC" | "GMT" => return Some(0),
        _ => {}
    }
    let (sign, body) = match s.as_bytes().first()? {
        b'+' => (1, &s[1..]),
        b'-' => (-1, &s[1..]),
        _ => return None,
    };
    let compact = body.replace(':', "");
    let h = num(compact.get(..2)?, 2)?;
    let m = num(compact.get(2..)?, 2)?;
    Some(sign * (h * 3600 + m * 60))
}

/// Days since 1970-01-01 for `YYYY-MM-DD` (proleptic Gregorian).
fn parse_date(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let y = num(&s[..4], 4)?;
    let m = num(&s[5..7], 2)?;
    let d = num(&s[8..10], 2)?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Howard Hinnant's days_from_civil.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_provider_formats() {
        assert_eq!(parse_timestamp("1970-01-01"), Some(0));
        assert_eq!(parse_timestamp("2024-03-01"), Some(1_709_251_200));
        assert_eq!(parse_timestamp("2023-09-08T08:00:00Z"), Some(1_694_160_000));
        assert_eq!(
            parse_timestamp("2023-09-08 08:00:00 UTC"),
            Some(1_694_160_000)
        );
        assert_eq!(
            parse_timestamp("2023-09-08 15:00:00 +0700"),
            Some(1_694_160_000)
        );
        assert_eq!(
            parse_timestamp("2023-09-08T15:00:00.123+07:00"),
            Some(1_694_160_000)
        );
        assert_eq!(
            parse_timestamp("2023-09-08T03:00:00-05:00"),
            Some(1_694_160_000)
        );
        assert_eq!(parse_timestamp("nope"), None);
        assert_eq!(parse_timestamp("2023-13-01"), None);
        assert_eq!(parse_timestamp("2023-09-08T25:00:00Z"), None);
        assert!(now_unix() > 1_700_000_000);
    }
}
