//! promise-prefer-await-to-then oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// Walk up ancestors and check if we sit inside an async function.
/// If yes, the `.then` could have been an `await`.
fn inside_async_function<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::Function(f) => return f.r#async,
            AstKind::ArrowFunctionExpression(a) => return a.r#async,
            _ => {}
        }
    }
    false
}

/// True when the receiver chain bottoms out at the literal identifier `z` (Zod).
/// Syntactic only — does not resolve aliased imports (`z as zod`), variable-bound schemas (`const Schema = z.string()`), or nested `.pipe(z.x)`.
fn receiver_is_zod_chain(expr: &Expression) -> bool {
    let mut cur = expr;
    loop {
        match cur {
            Expression::Identifier(id) => return id.name.as_str() == "z",
            Expression::StaticMemberExpression(m) => cur = &m.object,
            Expression::ComputedMemberExpression(m) => cur = &m.object,
            Expression::CallExpression(c) => cur = &c.callee,
            Expression::TSNonNullExpression(n) => cur = &n.expression,
            Expression::ParenthesizedExpression(p) => cur = &p.expression,
            _ => return false,
        }
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".then("])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "then" {
            return;
        }
        // Zod `.catch`/`.then` are schema combinators — flagging them is a false positive.
        if receiver_is_zod_chain(&member.object) {
            return;
        }
        // Only flag inside async functions — switching to await is
        // trivially possible there.
        if !inside_async_function(node, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.then(...)` inside an async function — replace with \
                      `const r = await expr; …`. Easier to read, simpler stack traces."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
