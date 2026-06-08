//! sql-advisory-lock-prefer-xact — oxc backend for TS / JS / TSX.

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
        if !super::uses_session_advisory_lock(&text) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, offset);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `pg_advisory_xact_lock()` instead of `pg_advisory_lock()` — \
                      it releases automatically at transaction end.".into(),
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
    fn flags_session_lock_in_string() {
        let src = r#"const q = "SELECT pg_advisory_lock(123)";"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_xact_lock() {
        let src = r#"const q = "SELECT pg_advisory_xact_lock(123)";"#;
        assert!(run_on(src).is_empty());
    }

    // Regression for #287: a session-level lock is the only variant that can
    // span a CREATE DATABASE (which cannot run inside a transaction block) — an
    // xact lock would be released before it runs.
    #[test]
    fn allows_session_lock_spanning_create_database() {
        let src = r#"const q = `psql -c "SELECT pg_advisory_lock(6210)" -c "CREATE DATABASE worker_db TEMPLATE shared_template"`;"#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }
}
