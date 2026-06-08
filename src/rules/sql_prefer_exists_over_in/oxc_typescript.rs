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
        if super::is_test_file(ctx.path) {
            return;
        }
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
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/api/features/users/user-scope.integration.test.ts");
        assert!(diags.is_empty());
    }
}
