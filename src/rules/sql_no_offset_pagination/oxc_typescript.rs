//! sql-no-offset-pagination — oxc backend for TS / JS / TSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::sql_helpers::is_sql_string;
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
        if !is_sql_string(&text) {
            return;
        }
        if !super::sql_uses_offset_pagination(&text) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, offset);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`OFFSET` pagination is O(N) on deep pages — use cursor-based pagination: `WHERE id > :last_id ORDER BY id LIMIT N`.".into(),
            severity: Severity::Warning,
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
    fn flags_offset_pagination() {
        let src = r#"const q = "SELECT * FROM users LIMIT 10 OFFSET 100";"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn does_not_flag_sql_without_offset() {
        let src = r#"const q = "SELECT * FROM users LIMIT 10";"#;
        assert!(run_on(src).is_empty());
    }

    /// Regression for #5557: an `OFFSET` in an expected-SQL oracle string inside
    /// a query-builder snapshot assertion (objection.js) is test data mirroring
    /// generated output, not a production query path. The central
    /// `skip_in_test_dir` gate suppresses the performance lint there.
    #[test]
    fn skips_offset_in_sql_snapshot_assertions_in_test_files() {
        let src = r#"
            QueryBuilder.forClass(TestModel)
              .where('test', 100)
              .range(100, 200)
              .then((res) => {
                expect(executedQueries).to.eql([
                  'select "Model".* from "Model" where "test" = 100 order by "order" asc limit 101 offset 100',
                ]);
              });
        "#;
        assert!(
            crate::rules::test_helpers::run_rule_gated(
                &Check,
                src,
                "tests/unit/queryBuilder/QueryBuilder.js"
            )
            .is_empty(),
            "must not flag OFFSET in SQL oracle strings under a tests/ directory"
        );
        assert!(
            crate::rules::test_helpers::run_rule_gated(
                &Check,
                src,
                "tests/unit/queryBuilder/QueryBuilder.spec.js"
            )
            .is_empty(),
            "must not flag OFFSET in SQL oracle strings in a .spec.js file"
        );
    }

    /// The performance lint still fires on OFFSET pagination in non-test source.
    #[test]
    fn still_fires_outside_test_dir() {
        let src = r#"const q = "SELECT * FROM users LIMIT 10 OFFSET 100";"#;
        assert_eq!(
            crate::rules::test_helpers::run_rule_gated(&Check, src, "src/db/users.ts").len(),
            1,
        );
    }
}
