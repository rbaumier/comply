//! sql-no-now-in-transaction — Rust backend.

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
        let upper = text.to_ascii_uppercase();
        if !(upper.contains("BEGIN") || upper.contains("START TRANSACTION")) {
            return;
        }
        if !(upper.contains("NOW()") || upper.contains("CURRENT_TIMESTAMP")) {
            return;
        }
        if !super::sql_uses_now_in_tx(text) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "`NOW()`/`CURRENT_TIMESTAMP` freezes at transaction start — use `clock_timestamp()` inside BEGIN blocks.".into(),
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
    fn flags_now_in_begin_block() {
        let src = r###"fn f() { let q = r#"BEGIN;
INSERT INTO log (ts) VALUES (NOW());
COMMIT;"#; }"###;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_clock_timestamp_in_tx() {
        let src = r###"fn f() { let q = r#"BEGIN;
INSERT INTO log (ts) VALUES (clock_timestamp());
COMMIT;"#; }"###;
        assert!(run(src).is_empty());
    }
}
