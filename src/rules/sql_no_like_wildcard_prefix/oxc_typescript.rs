//! sql-no-like-wildcard-prefix — oxc backend for TS / JS / TSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StringLiteral, AstType::TemplateLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (text, offset) = match node.kind() {
            AstKind::StringLiteral(lit) => (lit.value.as_str().to_string(), lit.span.start as usize),
            AstKind::TemplateLiteral(tpl) => {
                let s: String = tpl.quasis.iter().map(|q| q.value.raw.as_str()).collect::<Vec<_>>().join(" ");
                (s, tpl.span.start as usize)
            }
            _ => return,
        };
        if !super::has_filter_leading_wildcard_like(&text) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, offset);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`LIKE '%...'` forces a sequential scan — use TSVECTOR + GIN \
                      index with `@@` for full-text search.".into(),
            severity: Severity::Error,
            span: None,
        });
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_leading_wildcard() {
        let src = r#"const q = "SELECT * FROM t WHERE name LIKE '%test%'";"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_suffix_wildcard() {
        let src = r#"const q = "SELECT * FROM t WHERE name LIKE 'test%'";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_leading_wildcard_on_system_catalog() {
        // FP #5751: introspection over pg_class — tiny unindexed catalog, no
        // index to lose to the leading wildcard.
        let src = r#"const q = `
SELECT cls.relname AS name
FROM pg_index i
JOIN pg_class cls ON cls.oid = i.indexrelid
WHERE cls.relname LIKE '%_trgm'
`;"#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn still_flags_leading_wildcard_on_user_table() {
        // The performance premise holds for real user tables.
        let src = r#"const q = "SELECT * FROM products WHERE name LIKE '%foo'";"#;
        assert_eq!(run_on(src).len(), 1);
    }
}
