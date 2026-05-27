//! testing-no-conditional-assertion OXC backend.
//!
//! Flag `expect(...)` calls inside an `if`-statement body within a
//! `test()` / `it()` callback.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryExpression, BinaryOperator, Expression, Statement, UnaryOperator};
use std::sync::Arc;

fn is_type_narrowing(expr: &Expression) -> bool {
    match expr.without_parentheses() {
        Expression::CallExpression(call) => {
            if let Expression::StaticMemberExpression(member) = &call.callee {
                let m = member.property.name.as_str();
                return matches!(m, "isErr" | "isOk");
            }
            false
        }
        Expression::BinaryExpression(bin) => {
            matches!(bin.operator, BinaryOperator::Instanceof) || is_nullish_check(bin)
        }
        Expression::UnaryExpression(unary) => {
            matches!(unary.operator, UnaryOperator::LogicalNot)
                && is_type_narrowing(&unary.argument)
        }
        _ => false,
    }
}

fn is_nullish_check(bin: &BinaryExpression) -> bool {
    if !matches!(
        bin.operator,
        BinaryOperator::StrictInequality
            | BinaryOperator::StrictEquality
            | BinaryOperator::Inequality
            | BinaryOperator::Equality
    ) {
        return false;
    }
    is_nullish_literal(&bin.left) || is_nullish_literal(&bin.right)
}

fn is_nullish_literal(expr: &Expression) -> bool {
    matches!(expr.without_parentheses(), Expression::NullLiteral(_))
        || matches!(expr.without_parentheses(), Expression::Identifier(id) if id.name.as_str() == "undefined")
}

/// Source text of the `expect(ARG)` argument when `stmt` is an unconditional
/// truthiness assertion (`expect(ARG).toBe(true)` / `expect(ARG).toBeTruthy()`),
/// else `None`. Negated forms (`expect(ARG).not.toBe(true)`) return `None`.
fn truthy_assertion_target<'a>(stmt: &Statement<'a>, source: &'a str) -> Option<&'a str> {
    use oxc_span::GetSpan;
    let Statement::ExpressionStatement(es) = stmt else { return None };
    let Expression::CallExpression(call) = &es.expression else { return None };
    let Expression::StaticMemberExpression(matcher) = &call.callee else { return None };
    let asserts_truthy = match matcher.property.name.as_str() {
        "toBeTruthy" => true,
        "toBe" | "toEqual" | "toStrictEqual" => matches!(
            call.arguments.first().and_then(|a| a.as_expression()),
            Some(Expression::BooleanLiteral(b)) if b.value
        ),
        _ => false,
    };
    if !asserts_truthy {
        return None;
    }
    let Expression::CallExpression(expect_call) = &matcher.object else { return None };
    let Expression::Identifier(id) = &expect_call.callee else { return None };
    if id.name.as_str() != "expect" {
        return None;
    }
    let arg = expect_call.arguments.first()?.as_expression()?;
    let span = arg.span();
    Some(source[span.start as usize..span.end as usize].trim())
}

/// True when a statement preceding the `if` in the same block unconditionally
/// asserts the condition is truthy (`expect(cond).toBe(true)`), so the branch
/// is guaranteed taken — the `if` only narrows the type for the compiler.
fn block_has_truthy_guard<'a>(
    stmts: &oxc_allocator::Vec<'a, Statement<'a>>,
    if_stmt: &oxc_ast::ast::IfStatement<'a>,
    source: &'a str,
) -> bool {
    use oxc_span::GetSpan;
    let test = if_stmt.test.span();
    let cond_text = source[test.start as usize..test.end as usize].trim();
    for stmt in stmts.iter() {
        if stmt.span().start >= if_stmt.span.start {
            break;
        }
        if truthy_assertion_target(stmt, source) == Some(cond_text) {
            return true;
        }
    }
    false
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
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        // Must be a bare `expect(...)` call.
        let Expression::Identifier(ident) = &call.callee else { return };
        if ident.name.as_str() != "expect" {
            return;
        }

        // Walk ancestors: need both an if-statement body and a test/it call.
        let mut in_if_body = false;
        let mut in_test = false;
        let nodes = semantic.nodes();
        let mut cur_id = nodes.parent_id(node.id());
        loop {
            if cur_id == node.id() || cur_id == nodes.parent_id(cur_id) {
                break;
            }
            let parent_kind = nodes.kind(cur_id);
            match parent_kind {
                AstKind::IfStatement(if_stmt) => {
                    use oxc_span::GetSpan;
                    let guarded = match nodes.kind(nodes.parent_id(cur_id)) {
                        AstKind::BlockStatement(b) => block_has_truthy_guard(&b.body, if_stmt, ctx.source),
                        AstKind::FunctionBody(b) => block_has_truthy_guard(&b.statements, if_stmt, ctx.source),
                        AstKind::Program(p) => block_has_truthy_guard(&p.body, if_stmt, ctx.source),
                        _ => false,
                    };
                    if is_type_narrowing(&if_stmt.test) || guarded {
                        // Type narrowing (result.isErr(), instanceof, !== null) or a
                        // preceding unconditional assertion guarantees the branch —
                        // not conditional logic.
                    } else {
                        let test_span = if_stmt.test.span();
                        let call_span = call.span;
                        if call_span.start < test_span.start || call_span.start >= test_span.end {
                            in_if_body = true;
                        }
                    }
                }
                AstKind::CallExpression(ancestor_call) => {
                    if let Expression::Identifier(id) = &ancestor_call.callee {
                        let n = id.name.as_str();
                        if n == "test" || n == "it" {
                            in_test = true;
                        }
                    }
                }
                _ => {}
            }
            if in_if_body && in_test {
                break;
            }
            cur_id = nodes.parent_id(cur_id);
        }

        if in_if_body && in_test {
            let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "expect(...) inside an if-branch silently skips when the branch is not taken \u{2014} make the assertion unconditional.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_expect_inside_plain_if() {
        let src = "test('a', () => {\n  if (x > 0) { expect(x).toBeGreaterThan(0); }\n});";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Regression for #293: an `if` whose condition is guaranteed by a preceding
    // unconditional `expect(cond).toBe(true)` only narrows the type — the branch
    // is always taken, so the assertions inside it are not conditional.
    #[test]
    fn allows_expect_when_guarded_by_preceding_assertion() {
        let src = "test('a', () => {\n\
                     const exit = run(parseRouterOutput(raw));\n\
                     expect(Exit.isSuccess(exit)).toBe(true);\n\
                     if (Exit.isSuccess(exit)) {\n\
                       expect(exit.value).toHaveLength(2);\n\
                       expect(exit.value[0]).toEqual({ name: 'funnel-l1' });\n\
                     }\n\
                   });";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // A `.not.toBe(true)` (or any non-truthy matcher) does not guarantee the
    // branch — the assertion inside the `if` stays flagged.
    #[test]
    fn negated_assertion_is_not_a_guard() {
        let src = "test('a', () => {\n\
                     expect(cond).not.toBe(true);\n\
                     if (cond) { expect(value).toBe(1); }\n\
                   });";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }
}
