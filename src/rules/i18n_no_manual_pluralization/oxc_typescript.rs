//! OXC backend for i18n-no-manual-pluralization.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ConditionalExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ConditionalExpression(cond_expr) = node.kind() else { return };

        let cond_span = cond_expr.test.span();
        let cond_text = &ctx.source[cond_span.start as usize..cond_span.end as usize];

        if !cond_text.contains("count") && !cond_text.contains("length") && !cond_text.contains(".size") {
            return;
        }
        if !cond_text.contains("=== 1") && !cond_text.contains("== 1") && !cond_text.contains("> 1") {
            return;
        }

        let cons_span = cond_expr.consequent.span();
        let alt_span = cond_expr.alternate.span();
        let cons_text = &ctx.source[cons_span.start as usize..cons_span.end as usize];
        let alt_text = &ctx.source[alt_span.start as usize..alt_span.end as usize];

        if cons_text.starts_with("t(") && alt_text.starts_with("t(") {
            let (line, column) = byte_offset_to_line_col(ctx.source, cond_expr.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Use `t('key', { count })` for pluralization — manual ternaries break CLDR plural rules.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use super::Check;


    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }

    #[test]
    fn flags_manual_plural() {
        assert_eq!(run("count === 1 ? t('item') : t('items')").len(), 1);
    }

    #[test]
    fn allows_t_with_count() {
        assert!(run("t('item', { count })").is_empty());
    }

    #[test]
    fn allows_non_translation_ternary() {
        assert!(run("count === 1 ? 'item' : 'items'").is_empty());
    }
}
