//! sql-nullable-requires-comment — oxc backend for TS / JS / TSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::sql_helpers::is_sql_ddl;
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
        if !is_sql_ddl(&text) {
            return;
        }
        let (base_line, _) = byte_offset_to_line_col(ctx.source, offset);
        for line_offset in super::nullable_lines_without_comment(&text) {
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line: base_line + line_offset,
                column: 1,
                rule_id: super::META.id.into(),
                message: "Nullable column has no comment explaining why NULL is allowed.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
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
    fn flags_nullable_in_template_literal() {
        let src = "const q = `CREATE TABLE t (\n  deleted_at TIMESTAMP,\n)`;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_not_null() {
        let src = "const q = `CREATE TABLE t (\n  email TEXT NOT NULL,\n)`;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_embedded_ts_prose_issue_1003() {
        // "create ... type" in prose + a TS line that contains SQL-type substrings
        // ("Date"); not DDL, so nothing should fire.
        let src = "const dts = `Create a copy of this Type.\n  updatedAt: Date | null\n`;";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // Inline SQL DDL with two uncommented nullable columns (issue #1199 example).
    const TEST_SCHEMA: &str = "const schema = `\n  CREATE TABLE users (\n    id SERIAL PRIMARY KEY,\n    name TEXT,\n    deleted_at TIMESTAMP\n  )\n`;";

    #[test]
    fn skips_test_files_issue_1199() {
        // Test schemas exist to exercise queries, not to document nullable design.
        let diags = crate::rules::test_helpers::run_rule_gated(
            &Check,
            TEST_SCHEMA,
            "tests/seeder/pg.test.ts",
        );
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn still_fires_in_production_code_issue_1199() {
        // The same schema under a non-test path must still be flagged.
        let diags =
            crate::rules::test_helpers::run_rule_gated(&Check, TEST_SCHEMA, "src/db/schema.ts");
        assert!(!diags.is_empty(), "expected diagnostics in production code");
    }
}
