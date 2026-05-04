//! zod-trim-before-min OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

/// Walk back through a method-chain (CallExpression whose callee is a
/// StaticMemberExpression whose object is another CallExpression ...) and
/// collect every method name. Returns `None` if the chain does not bottom
/// out at a `z.string()` call.
fn collect_chain<'a>(expr: &'a Expression<'a>, ctx: &CheckCtx) -> Option<Vec<&'a str>> {
    let mut methods = Vec::new();
    let mut cur = expr;
    loop {
        let Expression::CallExpression(call) = cur else { return None };
        match &call.callee {
            Expression::StaticMemberExpression(member) => {
                let callee_text = &ctx.source[member.span.start as usize..member.span.end as usize];
                if callee_text == "z.string" {
                    return Some(methods);
                }
                methods.push(member.property.name.as_str());
                cur = &member.object;
            }
            _ => return None,
        }
    }
}

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

        // Only fire on the `.min(...)` call itself.
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "min" { return; }

        // The receiver chain must reach `z.string()`.
        let Some(methods) = collect_chain(&member.object, ctx) else { return };

        // If `.trim()` appears anywhere in the chain (before `.min`), no warning.
        if methods.contains(&"trim") { return; }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Add `.trim()` before `.min()` — `z.string().min(1)` allows whitespace-only strings.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
