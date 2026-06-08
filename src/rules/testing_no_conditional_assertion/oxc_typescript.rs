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

/// Extracts `(expect_arg_text, matcher_value_text)` from an equality assertion
/// like `expect(A).toBe(B)`, `expect(A).toEqual(B)`, `expect(A).toStrictEqual(B)`.
/// Returns `None` for negated forms or unrecognised matchers.
fn equality_assertion_parts<'a>(stmt: &Statement<'a>, source: &'a str) -> Option<(&'a str, &'a str)> {
    use oxc_span::GetSpan;
    let Statement::ExpressionStatement(es) = stmt else { return None };
    let Expression::CallExpression(call) = &es.expression else { return None };
    let Expression::StaticMemberExpression(matcher) = &call.callee else { return None };
    if !matches!(matcher.property.name.as_str(), "toBe" | "toEqual" | "toStrictEqual") {
        return None;
    }
    // The object must be a bare `expect(...)`, not `expect(...).not`
    let Expression::CallExpression(expect_call) = &matcher.object else { return None };
    let Expression::Identifier(id) = &expect_call.callee else { return None };
    if id.name.as_str() != "expect" {
        return None;
    }
    let expect_arg = expect_call.arguments.first()?.as_expression()?;
    let matcher_arg = call.arguments.first()?.as_expression()?;
    let expect_text = source[expect_arg.span().start as usize..expect_arg.span().end as usize].trim();
    let matcher_text = source[matcher_arg.span().start as usize..matcher_arg.span().end as usize].trim();
    Some((expect_text, matcher_text))
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
/// asserts the condition is truthy (`expect(cond).toBe(true)`) or asserts both
/// sides of an equality condition (`expect(A).toBe(B)` preceding `if (A === B)`),
/// so the branch is guaranteed taken — the `if` only narrows the type for the
/// compiler.
fn block_has_truthy_guard<'a>(
    stmts: &oxc_allocator::Vec<'a, Statement<'a>>,
    if_stmt: &oxc_ast::ast::IfStatement<'a>,
    source: &'a str,
) -> bool {
    use oxc_span::GetSpan;
    let test = if_stmt.test.span();
    let cond_text = source[test.start as usize..test.end as usize].trim();

    // When the condition is `A === B` or `A == B`, a preceding `expect(A).toBe(B)`
    // (or with sides swapped) guarantees the branch.
    let equality_sides: Option<(&str, &str)> = match if_stmt.test.without_parentheses() {
        Expression::BinaryExpression(bin)
            if matches!(
                bin.operator,
                BinaryOperator::StrictEquality | BinaryOperator::Equality
            ) =>
        {
            let left = source[bin.left.span().start as usize..bin.left.span().end as usize].trim();
            let right =
                source[bin.right.span().start as usize..bin.right.span().end as usize].trim();
            Some((left, right))
        }
        _ => None,
    };

    for stmt in stmts.iter() {
        if stmt.span().start >= if_stmt.span.start {
            break;
        }
        if truthy_assertion_target(stmt, source) == Some(cond_text) {
            return true;
        }
        if let Some((left, right)) = equality_sides {
            if let Some((exp_arg, mat_arg)) = equality_assertion_parts(stmt, source) {
                if (exp_arg == left && mat_arg == right) || (exp_arg == right && mat_arg == left) {
                    return true;
                }
            }
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
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

    // Regression for #514: `expect(A).toBe(B)` preceding `if (A === B)` means
    // the branch is guaranteed — the `if` is TypeScript type narrowing only.
    #[test]
    fn allows_expect_when_guarded_by_equality_assertion() {
        let src = "test('a', () => {\n\
                     expect(found?.level).toBe('team');\n\
                     if (found?.level === 'team') {\n\
                       expect(found.teams.map(m => m.id)).toEqual([team.id]);\n\
                     }\n\
                   });";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // `toEqual` variant of the equality guard.
    #[test]
    fn allows_expect_when_guarded_by_to_equal_assertion() {
        let src = "test('a', () => {\n\
                     expect(found?.level).toEqual('organization');\n\
                     if (found?.level === 'organization') {\n\
                       expect(found.organizations).toHaveLength(1);\n\
                     }\n\
                   });";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // A plain if without a guard is still flagged.
    #[test]
    fn flags_unguarded_equality_if() {
        let src = "test('a', () => {\n\
                     if (found?.level === 'team') {\n\
                       expect(found.teams).toHaveLength(1);\n\
                     }\n\
                   });";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }
}
