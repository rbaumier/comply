//! rust-prefer-unwrap-or-explicit backend.
//!
//! Flags `.unwrap_or_default()` method calls in non-test code. The
//! reader should be able to tell, at the call site, what value is
//! produced on `None`/`Err` without having to look up the `Default`
//! impl of the receiver's type. `.unwrap_or(<value>)` and
//! `.unwrap_or_else(|| <expr>)` both make the fallback visible; this
//! rule nudges authors toward one of those two forms.
//!
//! Bare `.unwrap()` / `.expect(...)` are intentionally out of scope —
//! they are handled by `rust-no-unwrap`. The two rules are independent
//! and cumulable.
//!
//! Tests are exempted via `is_in_test_context`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::is_in_test_context;

const KINDS: &[&str] = &["call_expression"];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        // Looking for `receiver.unwrap_or_default()`.
        let Some(function) = node.child_by_field_name("function") else {
            return;
        };
        if function.kind() != "field_expression" {
            return;
        }
        let Some(field) = function.child_by_field_name("field") else {
            return;
        };
        let Ok(field_text) = field.utf8_text(source_bytes) else {
            return;
        };
        if field_text != "unwrap_or_default" {
            return;
        }
        if is_in_test_context(node, source_bytes) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-prefer-unwrap-or-explicit".into(),
            message: "`.unwrap_or_default()` hides the fallback value from the reader. \
                      Write it explicitly: `.unwrap_or(<value>)` or \
                      `.unwrap_or_else(|| <expr>)`. The goal is that a reader should \
                      see what the code does on None/Err without looking up trait impls."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_unwrap_or_default() {
        assert_eq!(run_on("fn f() { let _ = x.unwrap_or_default(); }").len(), 1);
    }

    #[test]
    fn allows_unwrap_or_explicit() {
        assert!(run_on("fn f() { let _ = x.unwrap_or(0); }").is_empty());
    }

    #[test]
    fn allows_unwrap_or_else() {
        assert!(run_on("fn f() { let _ = x.unwrap_or_else(|| 0); }").is_empty());
    }

    #[test]
    fn does_not_flag_plain_unwrap() {
        assert!(run_on("fn f() { let _ = x.unwrap(); }").is_empty());
    }

    #[test]
    fn allows_in_test_context() {
        let source = "#[test]\nfn t() { let _ = x.unwrap_or_default(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_unrelated_method() {
        assert!(run_on("fn f() { let _ = x.default(); }").is_empty());
    }
}
