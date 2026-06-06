//! OXC backend for detect-dangerous-redirects.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

/// Returns true if `expr` is a member expression chain rooted at `req`,
/// e.g. `req.query.to`, `req.body.url`, `req.params.dest`.
fn is_req_member(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(id) => id.name == "req",
        Expression::StaticMemberExpression(m) => is_req_member(&m.object),
        Expression::ComputedMemberExpression(m) => is_req_member(&m.object),
        _ => false,
    }
}

fn arg_is_req_member(arg: &Argument) -> bool {
    match arg {
        Argument::Identifier(id) => id.name == "req",
        _ => {
            if let Some(expr) = arg.as_expression() {
                is_req_member(expr)
            } else {
                false
            }
        }
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["redirect"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Must be a `.redirect` method call (not a bare `redirect()`).
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "redirect" {
            return;
        }

        // Check if any argument is rooted at `req`.
        let tainted = call.arguments.iter().any(|arg| arg_is_req_member(arg));
        if !tainted {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Redirecting to a value from `req` enables open-redirect attacks — validate against an allowlist first.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}
