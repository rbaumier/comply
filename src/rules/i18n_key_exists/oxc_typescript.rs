//! i18n-key-exists OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Argument;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let callee_span = call.callee.span();
        let callee_text = &ctx.source[callee_span.start as usize..callee_span.end as usize];
        if callee_text != "t" && callee_text != "i18n.t" {
            return;
        }

        let Some(first_arg) = call.arguments.first() else { return };
        let Argument::StringLiteral(lit) = first_arg else { return };

        let inner = lit.value.as_str();
        if !super::is_malformed(inner) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, lit.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "t() key is malformed (consecutive/leading/trailing dots, empty segment, or non-alphanumeric character) — it cannot resolve to a locale entry.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_double_dot() {
        assert_eq!(run("t('auth..title')").len(), 1);
    }


    #[test]
    fn flags_trailing_dot() {
        assert_eq!(run("t('auth.title.')").len(), 1);
    }


    #[test]
    fn allows_normal_key() {
        assert!(run("t('auth.title')").is_empty());
    }


    #[test]
    fn flags_leading_dot() {
        assert_eq!(run("t('.auth.title')").len(), 1);
    }


    #[test]
    fn flags_slash_in_key() {
        assert_eq!(run("t('auth/title')").len(), 1);
    }


    #[test]
    fn flags_special_char_in_key() {
        assert_eq!(run("t('auth.title!')").len(), 1);
    }
}
