//! error-message-is-remediation — OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

const VERBS: &[&str] = &[
    "is", "are", "was", "were", "be", "been", "has", "have", "had", "do", "does", "did", "will",
    "would", "could", "should", "may", "might", "must", "shall", "can", "need", "check", "verify",
    "ensure", "provide", "specify", "use", "try", "retry", "pass", "set", "add", "remove",
    "update", "create", "delete", "call", "return", "expect", "require", "missing", "failed",
    "cannot", "unable", "exceeded", "denied", "rejected", "not",
];

fn has_verb(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    VERBS
        .iter()
        .any(|v| lower.split_whitespace().any(|w| w == *v))
}

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Error"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let oxc_ast::AstKind::NewExpression(new_expr) = node.kind() else {
            return;
        };

        // Check constructor name is "Error".
        let callee_name = match &new_expr.callee.without_parentheses() {
            Expression::Identifier(id) => &*id.name,
            _ => return,
        };
        if callee_name != "Error" {
            return;
        }

        // Get the first argument.
        let Some(first_arg) = new_expr.arguments.first() else {
            return;
        };
        let Some(arg_expr) = first_arg.as_expression() else {
            return;
        };

        // Extract string content.
        let source = semantic.source_text();
        let msg = match arg_expr.without_parentheses() {
            Expression::StringLiteral(s) => &*s.value,
            Expression::TemplateLiteral(t) => {
                // Only handle simple template literals (no expressions).
                if !t.expressions.is_empty() {
                    return;
                }
                if let Some(quasi) = t.quasis.first() {
                    &*quasi.value.raw
                } else {
                    return;
                }
            }
            _ => return,
        };

        let too_short = msg.len() < 15;
        let no_verb = !has_verb(msg);

        if too_short || no_verb {
            let (line, col) = byte_offset_to_line_col(source, new_expr.span().start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column: col,
                rule_id: super::META.id.into(),
                message: format!(
                    "Error message \"{msg}\" is too vague \
                     — describe what went wrong and what to do about it."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
