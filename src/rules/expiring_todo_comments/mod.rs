//! expiring-todo-comments — TODO/FIXME with expired date conditions.

mod rust;
mod text;
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "expiring-todo-comments",
    description: "TODO/FIXME with an expiration date that has passed should be resolved.",
    remediation: "Resolve the TODO/FIXME — the expiration date has passed. \
                  Either complete the task or update the date.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

/// Matches an ISO-8601 date `YYYY-MM-DD` anywhere in `s`.
fn find_date(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    if bytes.len() < 10 {
        return None;
    }
    for i in 0..=bytes.len() - 10 {
        if bytes[i].is_ascii_digit()
            && bytes[i + 1].is_ascii_digit()
            && bytes[i + 2].is_ascii_digit()
            && bytes[i + 3].is_ascii_digit()
            && bytes[i + 4] == b'-'
            && bytes[i + 5].is_ascii_digit()
            && bytes[i + 6].is_ascii_digit()
            && bytes[i + 7] == b'-'
            && bytes[i + 8].is_ascii_digit()
            && bytes[i + 9].is_ascii_digit()
        {
            return Some(&s[i..i + 10]);
        }
    }
    None
}

/// Parse `YYYY-MM-DD` into a comparable u32 `YYYYMMDD`.
fn parse_date_u32(date: &str) -> Option<u32> {
    if date.len() != 10 {
        return None;
    }
    let year: u32 = date[0..4].parse().ok()?;
    let month: u32 = date[5..7].parse().ok()?;
    let day: u32 = date[8..10].parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(year * 10000 + month * 100 + day)
}

/// Returns today as `YYYYMMDD` u32 — chrono-free.
pub(crate) fn today_u32() -> u32 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = (now / 86400) as i64;
    let (y, m, d) = days_to_ymd(days);
    (y as u32) * 10000 + (m as u32) * 100 + (d as u32)
}

/// Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
fn days_to_ymd(days: i64) -> (i32, i32, i32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64 + era * 400) as i32;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as i32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as i32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Locate a TODO/FIXME marker in `text` and return the `(marker, rest)`
/// after it. `text` is the full comment text (including markers).
fn find_todo_marker(text: &str) -> Option<(&'static str, &str)> {
    for marker in ["TODO", "FIXME"] {
        if let Some(pos) = text.find(marker) {
            return Some((marker, &text[pos + marker.len()..]));
        }
    }
    None
}

/// Find a `[...]` block immediately containing an ISO date, and report
/// whether it has expired.
fn check_expiration(rest: &str, today: u32) -> Option<(String, bool)> {
    let bracket_start = rest.find('[')?;
    let bracket_end = rest[bracket_start..].find(']')?;
    let args = &rest[bracket_start + 1..bracket_start + bracket_end];

    let date_str = find_date(args)?;
    let date_val = parse_date_u32(date_str)?;
    let expired = date_val <= today;
    Some((date_str.to_string(), expired))
}

/// Inspect a single comment node's text and return a diagnostic message
/// if the comment carries an expired TODO/FIXME date. The caller is
/// responsible for building the final `Diagnostic`.
pub(crate) fn check_comment_text(text: &str, today: u32) -> Option<String> {
    let (marker, rest) = find_todo_marker(text)?;
    let (date_str, expired) = check_expiration(rest, today)?;
    if !expired {
        return None;
    }
    Some(format!(
        "{marker} has expired (date {date_str} is past due) — \
         resolve it or update the expiration date."
    ))
}

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Tsx, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::JavaScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
            (Language::Vue, Backend::Text(Box::new(text::Check))),
        ],
    }
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn parse_date_valid() {
        assert_eq!(parse_date_u32("2024-12-31"), Some(20241231));
    }

    #[test]
    fn parse_date_invalid_month() {
        assert_eq!(parse_date_u32("2024-13-01"), None);
    }
}
