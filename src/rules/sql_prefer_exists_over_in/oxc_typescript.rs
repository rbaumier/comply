//! sql-prefer-exists-over-in — oxc backend for TS / JS / TSX.

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
        if !super::contains_in_subquery(&text) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, offset);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`IN (SELECT ...)` materializes the entire subquery — \
                      use `EXISTS (SELECT 1 ...)` which short-circuits on first match.".into(),
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
    fn flags_in_subquery() {
        let src = r#"const q = "WHERE id IN (SELECT user_id FROM orders)";"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_exists() {
        let src = r#"const q = "WHERE EXISTS (SELECT 1 FROM orders WHERE orders.user_id = u.id)";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_in_integration_test_teardown() {
        // Regression for #528: DELETE ... IN (SELECT ...) in test teardown files.
        let src = r#"const q = "DELETE FROM users WHERE id IN (SELECT id FROM temp_users)";"#;
        let diags = crate::rules::test_helpers::run_rule_gated(
            &Check,
            src,
            "src/api/features/users/user-scope.integration.test.ts",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn no_fp_in_query_builder_snapshot_assertion() {
        // Regression for #5556: an `IN (SELECT ...)` SQL string that is a snapshot
        // oracle inside an objection.js builder test under `tests/` (no `.test.`
        // infix) is expected output, not an executed query.
        let src = r#"expect(q).to.eql(['select "Model".* from "Model" where "A"."a" in (select "a" from "Model")']);"#;
        let diags = crate::rules::test_helpers::run_rule_gated(
            &Check,
            src,
            "tests/unit/queryBuilder/QueryBuilder.js",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn still_flags_in_subquery_in_production() {
        let src = r#"const q = "WHERE id IN (SELECT user_id FROM orders)";"#;
        let diags = crate::rules::test_helpers::run_rule_gated(&Check, src, "src/db/queries.ts");
        assert_eq!(diags.len(), 1);
    }
}
