//! react-require-versioned-storage-key oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn has_version_suffix(key: &str) -> bool {
    let Some(idx) = key.rfind(":v") else {
        return false;
    };
    let suffix = &key[idx + 2..];
    !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit())
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["localStorage"])
    }

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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Check callee is `localStorage.setItem`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name != "localStorage" || member.property.name.as_str() != "setItem" {
            return;
        }

        // Get first argument — must be a string literal.
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let Some(expr) = first_arg.as_expression() else {
            return;
        };
        let Expression::StringLiteral(lit) = expr else {
            return;
        };

        let key = lit.value.as_str();
        if has_version_suffix(key) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, lit.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Storage key `{key}` has no `:vN` version suffix \u{2014} bumping the \
                 version lets you migrate or drop old entries when the shape changes."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
