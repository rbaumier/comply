//! expiring-todo-comments backend — scan comments for TODO/FIXME with
//! expiration dates and flag those that have expired.
//!
//! Complementary to `todo-needs-issue-link`: that rule requires an issue
//! reference, this one checks for date-based expiration conditions like
//! `// TODO [2024-12-31]: migrate to v2`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Matches ISO-8601 dates: `YYYY-MM-DD`.
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

/// Returns today as `YYYYMMDD` u32.
fn today_u32() -> u32 {
    // Use a compile-time-friendly fallback; in practice the runtime date
    // matters.  We parse from chrono-free manual approach.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Convert epoch seconds to YYYYMMDD without chrono.
    // Days since epoch
    let days = (now / 86400) as i64;
    let (y, m, d) = days_to_ymd(days);
    (y as u32) * 10000 + (m as u32) * 100 + (d as u32)
}

/// Civil date from days since 1970-01-01 (Algorithm from Howard Hinnant).
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

/// Check if a line contains a TODO/FIXME marker inside a comment.
/// Returns `(marker, rest_of_comment)` if found.
fn find_todo_marker(line: &str) -> Option<(&'static str, &str)> {
    let comment_start = line.find("//").or_else(|| line.find("/*"))?;
    let after = &line[comment_start..];
    for marker in ["TODO", "FIXME"] {
        if let Some(pos) = after.find(marker) {
            return Some((marker, &after[pos + marker.len()..]));
        }
    }
    None
}

/// Check if a TODO/FIXME has a bracketed argument containing a date,
/// and if that date has expired.
fn check_expiration(rest: &str, today: u32) -> Option<(String, bool)> {
    // Look for `[...]` argument block after the marker
    let bracket_start = rest.find('[')?;
    let bracket_end = rest[bracket_start..].find(']')?;
    let args = &rest[bracket_start + 1..bracket_start + bracket_end];

    // Find a date in the arguments
    let date_str = find_date(args)?;
    let date_val = parse_date_u32(date_str)?;
    let expired = date_val <= today;
    Some((date_str.to_string(), expired))
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let today = today_u32();
        let mut diagnostics = Vec::new();

        for (idx, line) in ctx.source.lines().enumerate() {
            let Some((marker, rest)) = find_todo_marker(line) else {
                continue;
            };

            let Some((date_str, expired)) = check_expiration(rest, today) else {
                continue;
            };

            if expired {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "expiring-todo-comments".into(),
                    message: format!(
                        "{marker} has expired (date {date_str} is past due) — \
                         resolve it or update the expiration date."
                    ),
                    severity: Severity::Warning,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_expired_todo() {
        let diags = run("// TODO [2020-01-01]: migrate to v2");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("2020-01-01"));
        assert!(diags[0].message.contains("expired"));
    }

    #[test]
    fn flags_expired_fixme() {
        let diags = run("// FIXME [2021-06-15]: remove workaround");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("FIXME"));
    }

    #[test]
    fn allows_future_date() {
        assert!(run("// TODO [2099-12-31]: future task").is_empty());
    }

    #[test]
    fn allows_todo_without_date() {
        // No bracket block at all — not this rule's concern
        assert!(run("// TODO fix this later").is_empty());
    }

    #[test]
    fn allows_todo_with_non_date_bracket() {
        assert!(run("// TODO [needs-review]: check this").is_empty());
    }

    #[test]
    fn flags_expired_in_block_comment() {
        let diags = run("/* TODO [2019-03-01]: old task */");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn ignores_code_not_comment() {
        assert!(run("const date = '2020-01-01';").is_empty());
    }

    #[test]
    fn parse_date_valid() {
        assert_eq!(parse_date_u32("2024-12-31"), Some(20241231));
    }

    #[test]
    fn parse_date_invalid_month() {
        assert_eq!(parse_date_u32("2024-13-01"), None);
    }

    #[test]
    fn multiple_todos_on_separate_lines() {
        let src = "// TODO [2020-01-01]: first\n// TODO [2020-06-01]: second\n";
        assert_eq!(run(src).len(), 2);
    }
}
