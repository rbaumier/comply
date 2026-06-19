//! rust-duration-over-integer-with-unit backend.
//!
//! Walks struct fields (`field_declaration`) and function parameters
//! (`parameter`). A node is flagged when:
//!
//! 1. its name ends (case-insensitively) with a time-unit suffix
//!    like `_seconds`, `_ms`, `_days`, `_nanos`, ...
//! 2. its type text, trimmed, is one of the primitive integer types
//!    (`u8`..`u128`, `i8`..`i128`, `usize`, `isize`).
//!
//! A `*_ns`/`*_ms` value typed `f32`/`f64` is a floating-point mathematical
//! parameter (an EWMA decay factor, a continuous statistical estimate) where
//! the integer-based `Duration` does not apply, so float types are not flagged.
//!
//! Qualified types (`std::time::Duration`), generic wrappers
//! (`Option<u64>`), and aliases are deliberately not flagged — the
//! check is intentionally shallow to keep false positives low.
//! Names that mark an absolute time *coordinate* (a point on a timeline:
//! `julian_days`, `unix_seconds`, `created_at_seconds`, `*_timestamp`,
//! `*_epoch`) are exempted — `Duration` models an elapsed span, not an
//! absolute timeline point, so the suggestion would be wrong.
//! Test code is exempted via `is_in_test_context`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::is_in_test_context;

const SUFFIXES: &[&str] = &[
    "_seconds",
    "_secs",
    "_ms",
    "_millis",
    "_milliseconds",
    "_minutes",
    "_mins",
    "_hours",
    "_hrs",
    "_days",
    "_nanos",
    "_ns",
    "_nanoseconds",
    "_micros",
    "_us",
    "_microseconds",
];

/// Calendar-system / epoch prefixes that mark an absolute time *coordinate*
/// (a point on a timeline, e.g. a Julian Day Number or a Unix timestamp)
/// rather than an elapsed span. `Duration` models a non-negative elapsed span
/// and cannot represent an absolute timeline point, so the suggestion is wrong
/// for these names even though they carry a unit word.
const ABSOLUTE_TIME_PREFIXES: &[&str] = &["julian_", "gregorian_", "unix_", "epoch_"];

const INTEGER_TYPES: &[&str] = &[
    "u8", "u16", "u32", "u64", "u128", "i8", "i16", "i32", "i64", "i128", "usize", "isize",
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["field_declaration", "parameter"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        if is_in_test_context(node, source_bytes) {
            return;
        }
        // Struct field: `timeout_seconds: u32`.
        if node.kind() == "field_declaration"
            && let Some(name_node) = node.child_by_field_name("name")
            && let Some(type_node) = node.child_by_field_name("type")
            && let Ok(name) = name_node.utf8_text(source_bytes)
            && let Ok(type_text) = type_node.utf8_text(source_bytes)
            && has_time_unit_suffix(name)
            && is_integer_type(type_text)
        {
            diagnostics.push(make_diagnostic(ctx, node, name, type_text));
            return;
        }
        // Function parameter: `fn f(window_days: u32)`.
        if node.kind() == "parameter"
            && let Some(pattern) = node.child_by_field_name("pattern")
            && let Some(type_node) = node.child_by_field_name("type")
            && let Ok(name) = pattern.utf8_text(source_bytes)
            && let Ok(type_text) = type_node.utf8_text(source_bytes)
            && has_time_unit_suffix(name)
            && is_integer_type(type_text)
        {
            diagnostics.push(make_diagnostic(ctx, node, name, type_text));
        }
    }
}

fn has_time_unit_suffix(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    SUFFIXES.iter().any(|s| lower.ends_with(s)) && !is_absolute_time_coordinate(&lower)
}

/// True when a (lowercased) name marks an absolute time *coordinate* — a point
/// on a timeline (calendar date or timestamp) — rather than an elapsed span.
/// `Duration` only models the latter, so unit-suffixed names that are really
/// absolute coordinates must not be flagged.
fn is_absolute_time_coordinate(lower: &str) -> bool {
    ABSOLUTE_TIME_PREFIXES.iter().any(|p| lower.starts_with(p))
        || lower.contains("timestamp")
        || lower.contains("_since_epoch")
        || lower.ends_with("_epoch")
        || lower.contains("_at_")
        || lower.ends_with("_at")
}

fn is_integer_type(text: &str) -> bool {
    let trimmed = text.trim();
    INTEGER_TYPES.contains(&trimmed)
}

fn make_diagnostic(
    ctx: &CheckCtx,
    node: tree_sitter::Node,
    name: &str,
    type_text: &str,
) -> Diagnostic {
    let pos = node.start_position();
    Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "rust-duration-over-integer-with-unit".into(),
        message: format!(
            "Field/param `{name}: {type_text}` encodes a time unit in the name \
             but not the type. Use `std::time::Duration` so the unit is \
             type-checked — `window: Duration` can't be confused between \
             seconds and millis."
        ),
        severity: Severity::Warning,
        span: None,
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_struct_field_seconds_u32() {
        assert_eq!(run_on("struct S { timeout_seconds: u32 }").len(), 1);
    }

    #[test]
    fn flags_struct_field_ms_u64() {
        assert_eq!(run_on("struct S { retry_ms: u64 }").len(), 1);
    }

    #[test]
    fn flags_fn_parameter_days_u32() {
        assert_eq!(run_on("fn f(window_days: u32) {}").len(), 1);
    }

    #[test]
    fn allows_fn_parameter_minutes_f64() {
        // A float-typed unit-suffixed value is a math parameter (continuous
        // estimate), not a `Duration` candidate; the integer-based `Duration`
        // cannot represent it.
        assert!(run_on("fn f(duration_minutes: f64) {}").is_empty());
    }

    #[test]
    fn allows_duration_type() {
        let source = "use std::time::Duration; struct S { window: Duration }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_non_time_field() {
        assert!(run_on("struct S { count: u32, retries: u32 }").is_empty());
    }

    #[test]
    fn allows_time_field_without_suffix_match() {
        assert!(run_on("struct S { deadline: u64 }").is_empty());
    }

    #[test]
    fn allows_julian_days_absolute_coordinate() {
        // Julian Day Number is an absolute astronomical date coordinate, not a
        // span; `Duration` cannot represent it. Regression for #3915.
        assert!(run_on("fn f(julian_days: f64) {}").is_empty());
    }

    #[test]
    fn allows_gregorian_days_absolute_coordinate() {
        assert!(run_on("fn f(gregorian_days: u64) {}").is_empty());
    }

    #[test]
    fn allows_unix_seconds_absolute_coordinate() {
        assert!(run_on("struct S { unix_seconds: i64 }").is_empty());
    }

    #[test]
    fn allows_created_at_seconds_absolute_coordinate() {
        assert!(run_on("struct S { created_at_seconds: u64 }").is_empty());
    }

    #[test]
    fn allows_request_timestamp_ms_absolute_coordinate() {
        assert!(run_on("struct S { request_timestamp_ms: u64 }").is_empty());
    }

    #[test]
    fn flags_retry_after_ms_span() {
        // `_at` markers must not match `after`; this is a genuine elapsed span.
        assert_eq!(run_on("fn f(retry_after_ms: u64) {}").len(), 1);
    }

    #[test]
    fn flags_timeout_seconds_span() {
        assert_eq!(run_on("fn f(timeout_seconds: u64) {}").len(), 1);
    }

    #[test]
    fn allows_in_test_context() {
        let source = "#[cfg(test)]\nmod tests { struct S { timeout_ms: u32 } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_decay_ns_f64_math_parameter() {
        // tower's PeakEwma load balancer: `decay_ns: f64` is an EWMA decay
        // factor, a floating-point math parameter, not an integer time span.
        // Regression for #4443.
        assert!(run_on("struct S { decay_ns: f64 }").is_empty());
    }

    #[test]
    fn allows_rtt_ns_f64_math_parameter() {
        // tower's RttEstimate: `rtt_ns: f64` is a continuous latency estimate,
        // not a `Duration` candidate. Regression for #4443.
        assert!(run_on("struct S { rtt_ns: f64 }").is_empty());
    }

    #[test]
    fn allows_fn_parameter_ms_f32_math_parameter() {
        assert!(run_on("fn f(timeout_ms: f32) {}").is_empty());
    }

    #[test]
    fn flags_struct_field_window_days_u64() {
        assert_eq!(run_on("struct S { window_days: u64 }").len(), 1);
    }

    #[test]
    fn flags_fn_parameter_delay_ms_i64() {
        assert_eq!(run_on("fn f(delay_ms: i64) {}").len(), 1);
    }
}
