//! db-no-string-concat-sql — Rust backend.
//!
//! Detects `format!("SELECT ... {}", var)` style SQL injection. The
//! detection is anchored at the *format string* (first string literal
//! inside the macro's `token_tree`), never at the macro's full text.
//! Identifiers in the macro arguments are ignored, so
//! `format!("…: {}", String::from_utf8_lossy(stderr))` no longer
//! gets flagged just because `from_utf8_lossy` contains `from`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::is_sql_string;

const FORMAT_MACROS: &[&str] = &[
    "format",
    "format_args",
    "write",
    "writeln",
    "print",
    "println",
    "eprint",
    "eprintln",
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["macro_invocation"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(mac) = node.child_by_field_name("macro") else {
            return;
        };
        let Ok(mac_name) = mac.utf8_text(source_bytes) else {
            return;
        };
        if !FORMAT_MACROS.contains(&mac_name) {
            return;
        }
        let Some(format_string) = first_string_literal_in_macro(node, source_bytes) else {
            return;
        };
        if !is_sql_string(format_string) {
            return;
        }
        // Require interpolation (`{}` or `{...}` placeholders) — a
        // bare `format!("SELECT ...")` with no args is harmless
        // (caller could just have written a string literal).
        if !format_string.contains('{') {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "db-no-string-concat-sql".into(),
            message: "String interpolation with SQL keywords — use \
                      parameterized queries (`$1`, `?`) instead."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// Walk the macro invocation's children for the first string literal.
/// `format!("…", x, y)` exposes its arguments inside a `token_tree`
/// child. The first `string_literal` / `raw_string_literal` we find
/// is the format string.
fn first_string_literal_in_macro<'src>(
    node: tree_sitter::Node,
    source: &'src [u8],
) -> Option<&'src str> {
    let mut cursor = node.walk();
    let mut stack: Vec<tree_sitter::Node> = node.children(&mut cursor).collect();
    while let Some(child) = stack.pop() {
        if matches!(child.kind(), "string_literal" | "raw_string_literal") {
            // Strip the leading/trailing quote bytes for both `"…"` and
            // `r#"…"#` forms — `is_sql_string` doesn't care about the
            // delimiters, but stripping them keeps the search space
            // tight.
            return child.utf8_text(source).ok();
        }
        let mut sub = child.walk();
        for grand in child.children(&mut sub) {
            stack.push(grand);
        }
    }
    None
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
    fn flags_format_with_sql_select() {
        let src = r#"fn f(id: i32) { let q = format!("SELECT * FROM users WHERE id = {}", id); }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_format_with_sql_update() {
        let src = r#"fn f(id: i32) { let q = format!("UPDATE users SET name = '{}' WHERE id = 1", name); }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn does_not_flag_format_with_from_utf8_lossy_arg() {
        // The exact FP from the user's report. The format string is
        // not SQL; the arg expression contains `from_utf8_lossy`,
        // which used to fool the substring scan.
        let src = r#"fn f(stderr: &[u8]) -> String { format!("failed to parse oxlint JSON output. oxlint stderr: {}", String::from_utf8_lossy(stderr)) }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_static_sql_without_interpolation() {
        let src = r#"fn f() { let q = format!("SELECT * FROM users WHERE id = 1"); }"#;
        // No `{}` interpolation — caller could have written a string literal.
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_non_sql_format() {
        let src = r#"fn f(x: i32) { let s = format!("hello {}", x); }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_non_format_macro() {
        let src = r#"fn f() { vec!["SELECT * FROM users WHERE id = {}", "x"]; }"#;
        // `vec!` isn't a format macro; not our concern.
        assert!(run_on(src).is_empty());
    }
}
