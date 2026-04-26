//! sql-no-function-on-indexed-column — TS / JS / TSX backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::{is_sql_string, TS_STRING_KINDS};

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
        if !is_sql_string(text) {
            return;
        }
        let Some(func) = super::find_bad_func_in_where(text) else {
            return;
        };
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            format!(
                "`{func}` in WHERE defeats the index — normalize the column or add a functional index."
            ),
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
    fn flags_date_trunc() {
        let src = r#"const q = "SELECT id FROM log WHERE date_trunc('day', created_at) = '2024-01-01'";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_lower() {
        let src = r#"const q = "SELECT id FROM user WHERE LOWER(email) = 'a@b.c'";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_plain_column_comparison() {
        let src = r#"const q = "SELECT id FROM user WHERE email = 'a@b.c'";"#;
        assert!(run(src).is_empty());
    }
}
