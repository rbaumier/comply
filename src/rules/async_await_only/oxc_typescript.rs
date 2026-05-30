use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

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
        Some(&[".then(", ".catch("])
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

        let method = member.property.name.as_str();
        if method != "then" && method != "catch" {
            return;
        }

        // Zod `.catch`/`.then` are schema combinators — flagging them is a false positive.
        if receiver_is_zod_chain(&member.object) {
            return;
        }

        // React.lazy() requires a sync callback returning a Promise — the .then()
        // reshapes the module object and cannot be replaced with await.
        if crate::oxc_helpers::is_react_lazy_factory_then(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`.{method}()` chain — prefer `async`/`await` for readability."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
