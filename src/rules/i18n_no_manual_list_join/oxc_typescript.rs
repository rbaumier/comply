//! i18n-no-manual-list-join oxc backend — flag `.join(",")` / `.join(" and ")`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

fn is_locale_separator(inner: &str) -> bool {
    let trimmed = inner.trim();
    trimmed == ","
        || trimmed == ", "
        || trimmed.eq_ignore_ascii_case("and")
        || trimmed.eq_ignore_ascii_case(", and")
}

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

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "join" {
            return;
        }

        let Some(first_arg) = call.arguments.first() else { return };
        let Some(expr) = first_arg.as_expression() else { return };
        let Expression::StringLiteral(lit) = expr else { return };
        let inner = lit.value.as_str();
        if !is_locale_separator(inner) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Manual list join leaks English separators. Use `Intl.ListFormat` so commas and `and` translate.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
