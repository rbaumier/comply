//! testing-no-conditional-assertion OXC backend.
//!
//! Flag `expect(...)` calls inside an `if`-statement body within a
//! `test()` / `it()` callback.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::test_guard_helpers::{is_narrowing_guard, test_scope_statements};
use oxc_ast::ast::{Expression, Statement};
use std::sync::Arc;

/// True when `expr` is, or chains off, a bare `expect(...)` call — covering
/// `expect(a).toBe(b)`, `expect(a).to.equal(b)` (chai), and bare `expect(a)`.
/// Walks the call/member chain down to its root identifier.
fn is_expect_chain(expr: &Expression) -> bool {
    match expr.without_parentheses() {
        Expression::Identifier(id) => id.name.as_str() == "expect",
        Expression::CallExpression(call) => is_expect_chain(&call.callee),
        Expression::StaticMemberExpression(m) => is_expect_chain(&m.object),
        Expression::ComputedMemberExpression(m) => is_expect_chain(&m.object),
        _ => false,
    }
}

/// True when `stmt` is an expression statement that performs an `expect(...)`
/// assertion (any matcher form, including chai chains and bare property
/// assertions like `expect(x).to.be.undefined`).
fn statement_asserts(stmt: &Statement) -> bool {
    match stmt {
        Statement::ExpressionStatement(es) => is_expect_chain(&es.expression),
        _ => false,
    }
}

/// True when every statement-list of `branch` contains at least one assertion.
/// A branch is the consequent or alternate of an `if`; it is normally a
/// `BlockStatement` but may be a bare statement.
fn branch_asserts(branch: &Statement) -> bool {
    match branch {
        Statement::BlockStatement(block) => block.body.iter().any(statement_asserts),
        other => statement_asserts(other),
    }
}

/// True when `if_stmt` is an if/else(-if) chain in which EVERY arm asserts and
/// a final `else` is present, so one arm always executes an assertion — the
/// enclosed `expect(...)` is never silently skipped.
fn every_arm_asserts(if_stmt: &oxc_ast::ast::IfStatement) -> bool {
    if !branch_asserts(&if_stmt.consequent) {
        return false;
    }
    match &if_stmt.alternate {
        None => false,
        Some(Statement::IfStatement(else_if)) => every_arm_asserts(else_if),
        Some(alternate) => branch_asserts(alternate),
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
        let mut scope: Option<Vec<&Statement>> = None;
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
                    let guarded = every_arm_asserts(if_stmt)
                        || is_narrowing_guard(
                            if_stmt,
                            scope.get_or_insert_with(|| test_scope_statements(node.id(), semantic)),
                            ctx.source,
                        );
                    if guarded {
                        // An if/else chain whose every arm asserts (a final
                        // `else` present), or an `if` the enclosing test scope
                        // already narrows, both guarantee that an assertion
                        // fires — not conditional logic.
                    } else if !in_test {
                        // Only an `if` reached before crossing the enclosing
                        // `it()`/`test()` call sits inside the test body. Once
                        // `in_test` is set, the `if` wraps the test registration
                        // itself (e.g. dialect filtering around `describe()`) —
                        // test selection, not a conditional assertion: when the
                        // test runs, its assertions execute unconditionally.
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
                severity: Severity::Error,
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

    // Regression for #8213: the discriminant is fixed by an object-literal
    // assertion in a sibling `test()` of the same `describe` — the `if` only
    // narrows the union so `dataset.value` is callable, and the remediation the
    // rule prescribes does not type-check.
    #[test]
    fn allows_discriminant_guard_asserted_in_a_sibling_test() {
        let src = "describe('args', () => {\n\
                     const dataset = run();\n\
                     test('should return new function', () => {\n\
                       expect(dataset).toStrictEqual({ typed: true, value: expect.any(Function) });\n\
                     });\n\
                     test('should not throw error for valid args', () => {\n\
                       if (dataset.typed) { expect(dataset.value(123)).toBe(123); }\n\
                     });\n\
                   });";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // With no assertion anywhere on the discriminant the same shape is a
    // genuine conditional assertion.
    #[test]
    fn flags_discriminant_guard_never_asserted() {
        let src = "describe('args', () => {\n\
                     const dataset = run();\n\
                     test('should not throw error for valid args', () => {\n\
                       if (dataset.typed) { expect(dataset.value(123)).toBe(123); }\n\
                     });\n\
                   });";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // An assertion inside the guarded branch cannot prove the branch is taken.
    #[test]
    fn flags_guard_asserted_only_inside_its_own_branch() {
        let src = "test('x', () => {\n\
                     if (dataset.typed) { expect(dataset.typed).toBe(true); }\n\
                   });";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Regression for #1004: an `if` wrapping the `describe()`/`it()` registration
    // is test selection (dialect filtering) — when the test runs, every assertion
    // executes unconditionally.
    #[test]
    fn allows_if_wrapping_describe_registration() {
        let src = "for (const dialect of DIALECTS) {\n\
  if (dialect === 'mysql' || dialect === 'sqlite') {\n\
    describe('replace', () => {\n\
      it('inserts', async () => {\n\
        const result = await query.executeTakeFirst();\n\
        expect(result).to.be.instanceOf(InsertResult);\n\
        expect(result.insertId).to.be.a('bigint');\n\
      });\n\
    });\n\
  }\n\
}";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // An `if` directly wrapping the `it()` call is also test selection.
    #[test]
    fn allows_if_wrapping_it_registration() {
        let src = "if (cond) { it('x', () => { expect(a).toBe(b); }); }";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // An inner `if` inside the test body stays flagged even when an outer `if`
    // wraps the test registration.
    #[test]
    fn flags_inner_if_despite_outer_registration_if() {
        let src = "if (outer) {\n\
  it('x', () => {\n\
    if (inner) { expect(a).toBe(b); }\n\
  });\n\
}";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Regression for #1231: an `if/else` where BOTH branches assert is never
    // silent — one branch always executes, so every run fires an assertion.
    #[test]
    fn allows_if_else_when_both_branches_assert() {
        let src = "it('a', async () => {\n\
                     const result = await query.executeTakeFirst();\n\
                     if (dialect === 'mysql') {\n\
                       expect(result.numChangedRows).to.equal(1n);\n\
                     } else {\n\
                       expect(result.numChangedRows).to.be.undefined;\n\
                     }\n\
                   });";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Negative space: an `if` without an `else` can still silently skip the
    // assertion — it stays flagged.
    #[test]
    fn flags_if_without_else() {
        let src = "it('a', () => {\n\
                     if (c) { expect(a).toBe(1); }\n\
                   });";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Negative space: an `else` that does NOT assert leaves the consequent
    // assertion effectively conditional — it stays flagged.
    #[test]
    fn flags_if_else_when_else_does_not_assert() {
        let src = "it('a', () => {\n\
                     if (c) { expect(a).toBe(1); } else { log('skip'); }\n\
                   });";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Negative space: an `if/else-if/else` chain where the final arm does not
    // assert leaves the earlier assertions conditional — they stay flagged.
    #[test]
    fn flags_if_else_if_chain_with_non_asserting_final_arm() {
        let src = "it('a', () => {\n\
                     if (c) { expect(a).toBe(1); }\n\
                     else if (d) { expect(b).toBe(2); }\n\
                     else { log('skip'); }\n\
                   });";
        assert_eq!(run(src).len(), 2, "{:?}", run(src));
    }

    // An `if/else-if/else` chain where EVERY arm asserts is exempt.
    #[test]
    fn allows_if_else_if_chain_when_every_arm_asserts() {
        let src = "it('a', () => {\n\
                     if (c) { expect(a).toBe(1); }\n\
                     else if (d) { expect(b).toBe(2); }\n\
                     else { expect(e).toBe(3); }\n\
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
