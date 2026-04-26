//! sql-no-like-wildcard-prefix — TS / JS / TSX backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::TS_STRING_KINDS;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(TS_STRING_KINDS)
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
        if !super::has_leading_wildcard_like(text) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "`LIKE '%...'` forces a sequential scan — use TSVECTOR + GIN \
             index with `@@` for full-text search."
                .into(),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_leading_wildcard() {
        let src = r#"const q = "SELECT * FROM t WHERE name LIKE '%test%'";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_suffix_wildcard() {
        let src = r#"const q = "SELECT * FROM t WHERE name LIKE 'test%'";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_in_comment() {
        let src = "// LIKE '%test%'\nconst x = 1;";
        assert!(run(src).is_empty());
    }
}
