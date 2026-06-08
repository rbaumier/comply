//! OXC backend for drizzle-updated-at-on-update.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["updatedAt", "updated_at"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectProperty(prop) = node.kind() else { return };

        // Extract key name.
        let key_name = match &prop.key {
            oxc_ast::ast::PropertyKey::StaticIdentifier(ident) => ident.name.as_str(),
            oxc_ast::ast::PropertyKey::StringLiteral(lit) => lit.value.as_str(),
            _ => return,
        };

        if key_name != "updatedAt" && key_name != "updated_at" {
            return;
        }

        // Value must be a call expression.
        let oxc_ast::ast::Expression::CallExpression(_) = &prop.value else {
            return;
        };

        // Check that the full chain text contains `.$onUpdate(`.
        let value_span = prop.value.span();
        let chain_text = &ctx.source[value_span.start as usize..value_span.end as usize];
        if chain_text.contains(".$onUpdate(") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, prop.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`updatedAt` must chain `.$onUpdate(() => new Date())` so the column is refreshed on every update.".into(),
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
    fn flags_updated_at_without_on_update() {
        let src = "const t = { updatedAt: timestamp('updated_at').defaultNow() }";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_updated_at_with_on_update() {
        let src = "const t = { updatedAt: timestamp('updated_at').defaultNow().$onUpdate(() => new Date()) }";
        assert!(run(src).is_empty());
    }


    #[test]
    fn ignores_other_keys() {
        let src = "const t = { createdAt: timestamp('created_at') }";
        assert!(run(src).is_empty());
    }
}
