//! i18n-key-requires-domain-prefix OXC backend — flag t() keys missing a
//! domain prefix.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_valid_namespaced(key: &str) -> bool {
    if key.is_empty() {
        return false;
    }
    let segments: Vec<&str> = key.split('.').collect();
    if segments.len() < 2 {
        return false;
    }
    for seg in &segments {
        if seg.is_empty() {
            return false;
        }
        let mut chars = seg.chars();
        let first = chars.next().unwrap();
        if !first.is_ascii_lowercase() {
            return false;
        }
        for c in chars {
            if !c.is_ascii_alphanumeric() {
                return false;
            }
        }
    }
    true
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

        let func_name = match &call.callee {
            Expression::Identifier(id) => id.name.as_str(),
            Expression::StaticMemberExpression(m) => {
                if matches!(&m.object, Expression::Identifier(id) if id.name == "i18n")
                    && m.property.name == "t"
                {
                    "i18n.t"
                } else {
                    return;
                }
            }
            _ => return,
        };
        if func_name != "t" && func_name != "i18n.t" {
            return;
        }

        let Some(first_arg) = call.arguments.first() else { return };
        let Some(expr) = first_arg.as_expression() else { return };
        let Expression::StringLiteral(lit) = expr else { return };
        let inner = lit.value.as_str();
        if inner.is_empty() {
            return;
        }
        // Skip sentence-style keys
        if inner.contains(' ') {
            return;
        }
        if is_valid_namespaced(inner) {
            return;
        }

        let span = lit.span();
        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "t() key must match `domain.subkey` (lowercase-leading segments, dot-separated).".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
