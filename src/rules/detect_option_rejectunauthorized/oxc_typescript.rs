//! detect-option-rejectunauthorized oxc backend — flag
//! `{ rejectUnauthorized: false }` object properties.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, is_in_sslmode_no_verify_branch};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, PropertyKey};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["rejectUnauthorized"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectProperty(prop) = node.kind() else { return };

        let key_name = match &prop.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => return,
        };
        if key_name != "rejectUnauthorized" {
            return;
        }

        if !matches!(&prop.value, Expression::BooleanLiteral(b) if !b.value) {
            return;
        }

        // A database driver translating the user's explicit `sslmode=no-verify`
        // choice into `{ rejectUnauthorized: false }` is honoring a configurable
        // opt-out, not hardcoding an insecure default — don't flag it.
        if is_in_sslmode_no_verify_branch(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, prop.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`rejectUnauthorized: false` disables TLS certificate validation — remove it."
                .into(),
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
    fn flags_hardcoded_reject_unauthorized_false() {
        assert_eq!(run_on("const x = { rejectUnauthorized: false };").len(), 1);
    }

    #[test]
    fn allows_reject_unauthorized_true() {
        assert!(run_on("const x = { rejectUnauthorized: true };").is_empty());
    }

    #[test]
    fn allows_sslmode_no_verify_switch_case() {
        let src = r#"
            function toBoolean(value) {
              switch (value) {
                case 'disable': return false;
                case 'no-verify': return { rejectUnauthorized: false };
              }
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_sslmode_no_verify_if_branch() {
        let src = r#"
            if (this.ssl === 'no-verify') {
              this.ssl = { rejectUnauthorized: false };
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_hardcoded_false_in_unrelated_switch_case() {
        let src = r#"
            switch (value) {
              case 'whatever': return { rejectUnauthorized: false };
            }
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_sslmode_no_verify_ternary_consequent() {
        let src = "const ssl = mode === 'no-verify' ? { rejectUnauthorized: false } : { rejectUnauthorized: true };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_false_in_ternary_alternate() {
        let src = "const ssl = mode === 'no-verify' ? { rejectUnauthorized: true } : { rejectUnauthorized: false };";
        assert_eq!(run_on(src).len(), 1);
    }
}
