//! sql-no-truncate-in-app — oxc backend for TS / JS / TSX.

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
        if ctx.file.in_benchmark_dir() {
            return;
        }
        let (text, offset) = match node.kind() {
            AstKind::StringLiteral(lit) => (lit.value.as_str().to_string(), lit.span.start as usize),
            AstKind::TemplateLiteral(tpl) => {
                let s: String = tpl.quasis.iter().map(|q| q.value.raw.as_str()).collect::<Vec<_>>().join(" ");
                (s, tpl.span.start as usize)
            }
            _ => return,
        };
        if !super::sql_uses_truncate(&text) {
            return;
        }
        if !super::looks_like_sql_truncate(&text) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, offset);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`TRUNCATE` bypasses triggers and audit — use `DELETE FROM` instead.".into(),
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
    fn flags_truncate_table() {
        let src = r#"const q = "TRUNCATE TABLE users";"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_delete_from() {
        let src = r#"const q = "DELETE FROM users";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_tailwind_truncate_class() {
        let src = r#"const cls = "truncate flex items-center";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_truncate_in_benchmark_file_issue1497() {
        let src = r#"const q = "TRUNCATE TABLE users";"#;
        let found =
            crate::rules::test_helpers::run_rule_gated(&Check, src, "bench/reset.bench.ts");
        assert!(found.is_empty());
    }
}
