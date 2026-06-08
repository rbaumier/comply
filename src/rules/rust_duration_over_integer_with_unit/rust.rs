//! rust-duration-over-integer-with-unit backend.
//!
//! Walks struct fields (`field_declaration`) and function parameters
//! (`parameter`). A node is flagged when:
//!
//! 1. its name ends (case-insensitively) with a time-unit suffix
//!    like `_seconds`, `_ms`, `_days`, `_nanos`, ...
//! 2. its type text, trimmed, is one of the primitive numeric types
//!    (`u8`..`u128`, `i8`..`i128`, `usize`, `isize`, `f32`, `f64`).
//!
//! Qualified types (`std::time::Duration`), generic wrappers
//! (`Option<u64>`), and aliases are deliberately not flagged — the
//! check is intentionally shallow to keep false positives low.
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

const NUMERIC_TYPES: &[&str] = &[
    "u8", "u16", "u32", "u64", "u128", "i8", "i16", "i32", "i64", "i128", "usize", "isize", "f32",
    "f64",
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
            && is_numeric_type(type_text)
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
            && is_numeric_type(type_text)
        {
            diagnostics.push(make_diagnostic(ctx, node, name, type_text));
        }
    }
}

fn has_time_unit_suffix(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    SUFFIXES.iter().any(|s| lower.ends_with(s))
}

fn is_numeric_type(text: &str) -> bool {
    let trimmed = text.trim();
    NUMERIC_TYPES.contains(&trimmed)
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
    fn flags_fn_parameter_minutes_f64() {
        assert_eq!(run_on("fn f(duration_minutes: f64) {}").len(), 1);
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
    fn allows_in_test_context() {
        let source = "#[cfg(test)]\nmod tests { struct S { timeout_ms: u32 } }";
        assert!(run_on(source).is_empty());
    }
}
