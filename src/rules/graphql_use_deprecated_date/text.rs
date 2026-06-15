//! graphql-use-deprecated-date — every `@deprecated` directive must carry a
//! `deletionDate` argument holding a `YYYY-MM-DD` string, and that date must not
//! already be in the past.
//!
//! Three issues are reported per directive:
//! - **Missing**: the directive has no argument list, or no `deletionDate`
//!   argument inside it.
//! - **Invalid**: `deletionDate` is not a string literal, or its contents are
//!   not a valid `YYYY-MM-DD` calendar date.
//! - **Due**: the date is strictly before today (UTC), so the deprecated member
//!   should already have been removed.
//!
//! The directive name match is exact (`@deprecated`), and only top-level
//! arguments of that directive are inspected — nested object/list literals and
//! strings inside argument values are skipped.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@deprecated"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut scanner = Scanner::new(ctx.source, today_utc());
        scanner.scan(&mut |issue, offset| {
            let message = match issue {
                Issue::Missing => {
                    "The `@deprecated` directive is missing a `deletionDate` argument — add `deletionDate: \"YYYY-MM-DD\"` so the deprecation has a scheduled removal date.".to_string()
                }
                Issue::Invalid => {
                    "The `deletionDate` argument of `@deprecated` is not a valid date — it must be a string in the `YYYY-MM-DD` format (e.g. `\"2099-12-25\"`).".to_string()
                }
                Issue::Due => {
                    "The `@deprecated` deletion date has passed — remove the deprecated member or move its `deletionDate` into the future.".to_string()
                }
            };
            let (line, column) = line_col(ctx.source, offset);
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: "graphql-use-deprecated-date".into(),
                message,
                severity: Severity::Warning,
                span: None,
            });
        });
        diagnostics
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Issue {
    Missing,
    Invalid,
    Due,
}

/// A `YYYY-MM-DD` calendar date, compared as a tuple. No timezone — the rule
/// only ever compares two UTC civil dates for ordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CivilDate {
    year: i32,
    month: u8,
    day: u8,
}

struct Scanner<'a> {
    src: &'a [u8],
    text: &'a str,
    i: usize,
    today: CivilDate,
}

impl<'a> Scanner<'a> {
    fn new(text: &'a str, today: CivilDate) -> Self {
        Scanner { src: text.as_bytes(), text, i: 0, today }
    }

    /// Walk the source, skipping strings/comments, and report each
    /// `@deprecated` directive whose `deletionDate` argument is missing,
    /// malformed, or past due.
    fn scan(&mut self, report: &mut dyn FnMut(Issue, usize)) {
        while self.i < self.src.len() {
            let b = self.src[self.i];
            match b {
                b'#' => self.skip_comment(),
                b'"' => self.skip_string(),
                b'@' => self.scan_directive(report),
                _ => self.i += 1,
            }
        }
    }

    /// At an `@`: read the directive name. If it is `deprecated`, inspect its
    /// argument list. The `@` position is reported as the diagnostic site.
    fn scan_directive(&mut self, report: &mut dyn FnMut(Issue, usize)) {
        let at = self.i;
        self.i += 1; // consume '@'
        let name = self.read_name();
        if name != "deprecated" {
            return;
        }
        // Whitespace and comments are allowed between the name and the `(`.
        self.skip_trivia();
        if self.i >= self.src.len() || self.src[self.i] != b'(' {
            report(Issue::Missing, at);
            return;
        }
        match self.find_deletion_date() {
            None => report(Issue::Missing, at),
            Some(DateValue::NotAString) => report(Issue::Invalid, at),
            Some(DateValue::Str(raw)) => match parse_iso_date(raw) {
                None => report(Issue::Invalid, at),
                Some(date) if date < self.today => report(Issue::Due, at),
                Some(_) => {}
            },
        }
    }

    /// Scan the directive argument list starting at the current `(` and return
    /// the value of the first top-level `deletionDate` argument, if any. Nested
    /// brackets and strings inside other argument values are skipped.
    fn find_deletion_date(&mut self) -> Option<DateValue<'a>> {
        debug_assert_eq!(self.src[self.i], b'(');
        self.i += 1; // consume '('
        let mut expect_name = true;
        let mut found: Option<DateValue<'a>> = None;
        while self.i < self.src.len() {
            let b = self.src[self.i];
            match b {
                b')' => {
                    self.i += 1;
                    break;
                }
                b'#' => self.skip_comment(),
                b'"' => self.skip_string(),
                b'{' | b'[' | b'(' => self.skip_balanced(),
                b',' => {
                    expect_name = true;
                    self.i += 1;
                }
                _ if expect_name && is_name_start(b) => {
                    let name = self.read_name();
                    self.skip_trivia();
                    let is_target = name == "deletionDate"
                        && self.i < self.src.len()
                        && self.src[self.i] == b':';
                    if is_target {
                        self.i += 1; // consume ':'
                        self.skip_trivia();
                        if found.is_none() {
                            found = Some(self.read_argument_value());
                        }
                    }
                    expect_name = false;
                }
                _ => self.i += 1,
            }
        }
        found
    }

    /// Read one argument value at the current position. A `"…"` string literal
    /// yields its inner text; anything else is `NotAString`.
    fn read_argument_value(&mut self) -> DateValue<'a> {
        if self.i < self.src.len() && self.src[self.i] == b'"' && !self.starts_with("\"\"\"") {
            let start = self.i + 1;
            self.skip_string();
            // `skip_string` left `self.i` one past the closing quote.
            let end = self.i.saturating_sub(1);
            if end >= start {
                return DateValue::Str(&self.text[start..end]);
            }
        }
        DateValue::NotAString
    }

    /// Skip a balanced bracket group starting at the current opener.
    fn skip_balanced(&mut self) {
        let open = self.src[self.i];
        let close = match open {
            b'(' => b')',
            b'{' => b'}',
            b'[' => b']',
            _ => return,
        };
        let mut depth = 0i32;
        while self.i < self.src.len() {
            let b = self.src[self.i];
            match b {
                b'#' => {
                    self.skip_comment();
                    continue;
                }
                b'"' => {
                    self.skip_string();
                    continue;
                }
                x if x == open => depth += 1,
                x if x == close => {
                    depth -= 1;
                    if depth == 0 {
                        self.i += 1;
                        return;
                    }
                }
                _ => {}
            }
            self.i += 1;
        }
    }

    fn read_name(&mut self) -> &'a str {
        let start = self.i;
        while self.i < self.src.len() && is_name_continue(self.src[self.i]) {
            self.i += 1;
        }
        &self.text[start..self.i]
    }

    fn skip_comment(&mut self) {
        while self.i < self.src.len() && self.src[self.i] != b'\n' {
            self.i += 1;
        }
    }

    fn skip_string(&mut self) {
        if self.starts_with("\"\"\"") {
            self.i += 3;
            while self.i < self.src.len() && !self.text[self.i..].starts_with("\"\"\"") {
                self.i += 1;
            }
            self.i = (self.i + 3).min(self.src.len());
            return;
        }
        self.i += 1; // opening quote
        while self.i < self.src.len() {
            match self.src[self.i] {
                b'\\' => self.i += 2,
                b'"' => {
                    self.i += 1;
                    return;
                }
                b'\n' => return,
                _ => self.i += 1,
            }
        }
    }

    /// Skip whitespace and `#` comments — the trivia allowed between tokens.
    fn skip_trivia(&mut self) {
        while self.i < self.src.len() {
            let b = self.src[self.i];
            if (b as char).is_whitespace() {
                self.i += 1;
            } else if b == b'#' {
                self.skip_comment();
            } else {
                break;
            }
        }
    }

    fn starts_with(&self, s: &str) -> bool {
        self.text[self.i..].starts_with(s)
    }
}

enum DateValue<'a> {
    Str(&'a str),
    NotAString,
}

fn is_name_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_name_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Parse a strict `YYYY-MM-DD` calendar date. Returns `None` on any format
/// deviation (non-ASCII-digit components, wrong separators, out-of-range
/// month/day, extra trailing text), matching a real date parse.
fn parse_iso_date(s: &str) -> Option<CivilDate> {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let year: i32 = parse_digits(&b[0..4])?;
    let month: u8 = parse_digits(&b[5..7])?;
    let day: u8 = parse_digits(&b[8..10])?;
    if !(1..=12).contains(&month) {
        return None;
    }
    if day < 1 || day > days_in_month(year, month) {
        return None;
    }
    Some(CivilDate { year, month, day })
}

/// Parse a fixed-width run of ASCII digits into an integer. Any non-digit byte
/// or overflow makes the whole value invalid.
fn parse_digits<T: TryFrom<u32>>(digits: &[u8]) -> Option<T> {
    let mut acc: u32 = 0;
    for &d in digits {
        if !d.is_ascii_digit() {
            return None;
        }
        acc = acc.checked_mul(10)?.checked_add(u32::from(d - b'0'))?;
    }
    T::try_from(acc).ok()
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Today's date in UTC, derived from the system clock (the days elapsed since
/// the Unix epoch converted to a civil date). Matches Biome's UTC comparison.
fn today_utc() -> CivilDate {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    civil_from_unix_days((secs / 86_400) as i64)
}

/// Convert a day count since 1970-01-01 (UTC) to a civil date using Howard
/// Hinnant's `civil_from_days` algorithm.
fn civil_from_unix_days(z: i64) -> CivilDate {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = (y + i64::from(m <= 2)) as i32;
    CivilDate { year, month: m as u8, day: d as u8 }
}

fn line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Run the rule against `source` and compare diagnostics ordering by issue,
    /// independent of the wall clock. The "Due" verdict for past dates and the
    /// "no diagnostic" verdict for far-future dates are stable regardless of
    /// when the suite runs (within the next ~75 years).
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("op.graphql"), source))
    }

    // --- Biome invalid.graphql fixtures ---

    #[test]
    fn missing_deletion_date_fires() {
        // Biome invalid case 1: only a `reason`, no `deletionDate`.
        let src = "query {\n  member @deprecated(reason: \"Use `members` instead\") {\n    id\n  }\n}";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("missing a `deletionDate`"), "{}", d[0].message);
    }

    #[test]
    fn malformed_deletion_date_fires_as_invalid() {
        // Biome invalid case 2: `deletionDate: "invalid-date"`.
        let src = "query {\n  member @deprecated(reason: \"Use `members` instead\", deletionDate: \"invalid-date\") {\n    id\n  }\n}";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("not a valid date"), "{}", d[0].message);
    }

    #[test]
    fn past_deletion_date_fires_as_due() {
        // Biome invalid case 3: `deletionDate: "1999-12-25"` is in the past.
        let src = "query {\n  member @deprecated(reason: \"Use `members` instead\", deletionDate: \"1999-12-25\") {\n    id\n  }\n}";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("has passed"), "{}", d[0].message);
    }

    // --- Biome valid.graphql fixture ---

    #[test]
    fn future_deletion_date_is_clean() {
        // Biome valid case: `deletionDate: "2099-12-25"` is in the future.
        let src = "query {\n  member @deprecated(reason: \"Use `members` instead\", deletionDate: \"2099-12-25\") {\n    id\n  }\n}";
        assert!(run(src).is_empty());
    }

    // --- Over-firing / scope guards ---

    #[test]
    fn directive_with_no_arguments_fires_missing() {
        let src = "type T {\n  old: String @deprecated\n  new: String\n}";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("missing a `deletionDate`"));
    }

    #[test]
    fn deprecated_substring_directive_is_ignored() {
        // `@deprecatedField` is a different directive — must not fire.
        let src = "type T {\n  f: String @deprecatedField(reason: \"x\")\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn other_directives_are_ignored() {
        let src = "query Q {\n  user @include(if: true) {\n    id\n  }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn deletion_date_in_comment_does_not_count() {
        // The arg list has no real `deletionDate`; the one in the comment is trivia.
        let src = "type T {\n  old: String @deprecated(\n    reason: \"x\" # deletionDate: \"2099-12-25\"\n  )\n}";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("missing a `deletionDate`"));
    }

    #[test]
    fn non_string_deletion_date_fires_invalid() {
        let src = "type T {\n  old: String @deprecated(deletionDate: 20991225)\n}";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("not a valid date"));
    }

    #[test]
    fn whitespace_between_name_and_args_is_handled() {
        let src = "type T {\n  old: String @deprecated  (reason: \"x\")\n}";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("missing a `deletionDate`"));
    }

    #[test]
    fn deletion_date_inside_nested_object_value_is_not_top_level() {
        // A `deletionDate` key nested in another argument's object literal is
        // not the directive's own argument and must not satisfy the rule.
        let src = "type T {\n  old: String @deprecated(reason: \"x\", meta: { deletionDate: \"2099-12-25\" })\n}";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("missing a `deletionDate`"));
    }

    #[test]
    fn multiple_deprecated_directives_each_reported() {
        let src = "type T {\n  a: String @deprecated(reason: \"x\")\n  b: String @deprecated(reason: \"y\")\n}";
        assert_eq!(run(src).len(), 2);
    }

    // --- parse_iso_date unit coverage ---

    #[test]
    fn parses_valid_iso_date() {
        assert_eq!(parse_iso_date("2099-12-25"), Some(CivilDate { year: 2099, month: 12, day: 25 }));
    }

    #[test]
    fn rejects_malformed_dates() {
        assert_eq!(parse_iso_date("invalid-date"), None);
        assert_eq!(parse_iso_date("2099-13-01"), None); // month out of range
        assert_eq!(parse_iso_date("2099-02-30"), None); // day out of range
        assert_eq!(parse_iso_date("2099-2-5"), None); // not zero-padded / wrong length
        assert_eq!(parse_iso_date("2099/12/25"), None); // wrong separator
        assert_eq!(parse_iso_date("2099-12-25 "), None); // trailing space
    }

    #[test]
    fn accepts_leap_day_only_in_leap_years() {
        assert!(parse_iso_date("2096-02-29").is_some()); // leap year
        assert_eq!(parse_iso_date("2099-02-29"), None); // not a leap year
    }

    #[test]
    fn civil_date_epoch_round_trips() {
        assert_eq!(civil_from_unix_days(0), CivilDate { year: 1970, month: 1, day: 1 });
        assert_eq!(civil_from_unix_days(18_993), CivilDate { year: 2022, month: 1, day: 1 });
    }
}
