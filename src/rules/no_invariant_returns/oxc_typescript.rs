//! no-invariant-returns OXC backend — flag functions that always return the
//! same literal value.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, Statement};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function, AstType::ArrowFunctionExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (span_start, func_span, body_stmts) = match node.kind() {
            AstKind::Function(func) => {
                let Some(body) = func.body.as_ref() else {
                    return;
                };
                (func.span.start, func.span, body.statements.as_slice())
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                if arrow.expression {
                    return; // concise body — single expression
                }
                (arrow.span.start, arrow.span, arrow.body.statements.as_slice())
            }
            _ => return,
        };

        // Collect return-statement literal values that belong directly to this function
        let nodes = semantic.nodes();
        let mut literals: Vec<String> = Vec::new();

        for snode in nodes.iter() {
            let AstKind::ReturnStatement(ret) = snode.kind() else {
                continue;
            };
            // Span check
            if ret.span.start < func_span.start || ret.span.end > func_span.end {
                continue;
            }
            // Must belong directly to this function, not a nested one
            if nearest_function_span(snode.id(), nodes) != Some(func_span) {
                continue;
            }

            let Some(arg) = &ret.argument else {
                // bare `return;` — non-literal, bail
                return;
            };
            match literal_text(arg) {
                Some(text) => literals.push(text),
                None => return, // non-literal return — can't prove invariance
            }
        }

        if literals.len() < 2 {
            return;
        }

        let first = &literals[0];
        if !literals.iter().all(|l| l == first) {
            return;
        }

        // If control can reach the end of the function body without hitting an
        // explicit `return`/`throw`, the function also returns `undefined`
        // implicitly — a distinct value, so it is not invariant. Only flag when
        // the body provably diverges on every path (#3221).
        if !block_always_diverges(body_stmts) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Function always returns the same literal value \u{2014} consider using a constant instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn nearest_function_span(
    node_id: oxc_semantic::NodeId,
    nodes: &oxc_semantic::AstNodes,
) -> Option<oxc_span::Span> {
    for kind in nodes.ancestor_kinds(node_id).skip(1) {
        match kind {
            AstKind::Function(f) => return Some(f.span),
            AstKind::ArrowFunctionExpression(a) => return Some(a.span),
            _ => {}
        }
    }
    None
}

/// Whether a sequence of statements always completes abruptly (every path ends
/// in a `return`/`throw`), so control cannot fall off the end. A conservative
/// approximation: only the last statement is examined, and any construct not
/// modeled below is treated as able to fall through (so the function is left
/// alone). Empty blocks fall through.
fn block_always_diverges(stmts: &[Statement]) -> bool {
    stmts.last().is_some_and(stmt_always_diverges)
}

fn stmt_always_diverges(stmt: &Statement) -> bool {
    match stmt {
        Statement::ReturnStatement(_) | Statement::ThrowStatement(_) => true,
        Statement::BlockStatement(block) => block_always_diverges(&block.body),
        // An `if` diverges only when it has an `else` and both branches diverge;
        // a bare `if` can fall through when the test is false.
        Statement::IfStatement(if_stmt) => {
            if_stmt.alternate.as_ref().is_some_and(|alt| {
                stmt_always_diverges(&if_stmt.consequent) && stmt_always_diverges(alt)
            })
        }
        // A `try` diverges when the `finally` diverges, or when the `try` block
        // diverges and any `catch` also diverges (so neither normal completion
        // nor a caught error can fall through).
        Statement::TryStatement(try_stmt) => {
            if try_stmt
                .finalizer
                .as_ref()
                .is_some_and(|f| block_always_diverges(&f.body))
            {
                return true;
            }
            let try_diverges = block_always_diverges(&try_stmt.block.body);
            let handler_diverges = try_stmt
                .handler
                .as_ref()
                .is_none_or(|h| block_always_diverges(&h.body.body));
            try_diverges && handler_diverges
        }
        _ => false,
    }
}

fn literal_text(expr: &Expression) -> Option<String> {
    match expr {
        Expression::NumericLiteral(n) => Some(
            n.raw
                .as_ref()
                .map_or_else(|| n.value.to_string(), |r| r.to_string()),
        ),
        Expression::StringLiteral(s) => Some(format!("\"{}\"", s.value)),
        Expression::BooleanLiteral(b) => Some(b.value.to_string()),
        Expression::NullLiteral(_) => Some("null".into()),
        Expression::Identifier(id) if id.name.as_str() == "undefined" => {
            Some("undefined".into())
        }
        _ => None,
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_invariant_with_explicit_trailing_return() {
        // Every path returns the same literal and the body ends in a return —
        // genuinely invariant, must still flag.
        let src = r#"
function classify(c) {
    if (c) {
        return "X";
    }
    return "X";
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_implicit_undefined_fall_through() {
        // Regression for #3221: guard returns a literal, the success path falls
        // through to an implicit `undefined`, the catch returns the same literal.
        // The implicit `undefined` is a distinct return value — not invariant.
        let src = r#"
async function addItem(prevState, selectedVariantId) {
    if (!selectedVariantId) {
        return "Error adding item to cart";
    }

    try {
        await addToCart([{ merchandiseId: selectedVariantId, quantity: 1 }]);
        updateTag("cart");
    } catch (e) {
        return "Error adding item to cart";
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_guard_clause_then_fall_through() {
        // Guard returns a literal, then the body falls through to implicit
        // `undefined` — not invariant.
        let src = r#"
function maybe(x) {
    if (x) {
        return "X";
    }
    doWork();
}
"#;
        assert!(run_on(src).is_empty());
    }
}
