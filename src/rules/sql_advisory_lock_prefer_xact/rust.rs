//! sql-advisory-lock-prefer-xact — Rust backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::RUST_STRING_KINDS;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(RUST_STRING_KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Ok(text) = node.utf8_text(source_bytes) else {
            return;
        };
        if !super::uses_session_advisory_lock(text) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "Use `pg_advisory_xact_lock()` instead of `pg_advisory_lock()` — \
             it releases automatically at transaction end."
                .into(),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(src, &Check)
    }

    #[test]
    fn flags_session_lock() {
        let src = r#"fn f() { let q = "SELECT pg_advisory_lock(123)"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_xact_lock() {
        let src = r#"fn f() { let q = "SELECT pg_advisory_xact_lock(123)"; }"#;
        assert!(run(src).is_empty());
    }
}
