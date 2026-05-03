//! no-conditional-tests oxc backend — flag `describe`/`test`/`it` calls wrapped
//! in conditional control flow (if, ternary, switch case).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const TEST_FNS: &[&str] = &["describe", "test", "it"];

pub struct Check;

fn callee_base_name<'a>(callee: &'a Expression<'a>) -> Option<&'a str> {
    match callee {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(member) => {
            // e.g. `test.each`, `describe.only` — extract the object identifier
            if let Expression::Identifier(obj) = &member.object {
                Some(obj.name.as_str())
            } else {
                None
            }
        }
        Expression::CallExpression(inner_call) => {
            // e.g. `test.each([1])('a', ...)` — the outer callee is a call
            callee_base_name(&inner_call.callee)
        }
        _ => None,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let Some(base) = callee_base_name(&call.callee) else { return };
        if !TEST_FNS.contains(&base) {
            return;
        }

        // Walk ancestors looking for conditional control flow.
        for ancestor_kind in semantic.nodes().ancestor_kinds(node.id()) {
            match ancestor_kind {
                AstKind::IfStatement(_)
                | AstKind::ConditionalExpression(_)
                | AstKind::SwitchStatement(_) => {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, call.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Don't conditionally define tests, use test.skip or describe.skip"
                            .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                    return;
                }
                _ => {}
            }
        }
    }
}
