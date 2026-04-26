//! sql-no-pg-enum — Rust backend.

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
        if !super::declares_pg_enum(text) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "PostgreSQL `CREATE TYPE ... AS ENUM` is append-only — \
             you can't remove values. Use `TEXT CHECK(col IN (...))` \
             or a lookup table instead."
                .into(),
            Severity::Error,
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
    fn flags_create_type_as_enum() {
        let src = r#"fn f() { let q = "CREATE TYPE status AS ENUM ('a', 'b')"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_text_check() {
        let src = r#"fn f() { let q = "status TEXT CHECK(status IN ('a', 'b'))"; }"#;
        assert!(run(src).is_empty());
    }
}
