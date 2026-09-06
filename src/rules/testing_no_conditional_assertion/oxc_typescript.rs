//! testing-no-conditional-assertion OXC backend.
//!
//! Flag the `if` chain inside a `test()` / `it()` callback that holds the
//! test's only assertions: when no arm runs, the test passes having checked
//! nothing. One diagnostic per chain, anchored on the `if`.

use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::test_guard_helpers::{is_narrowing_guard, is_type_narrowing, test_scope_statements};
use oxc_ast::ast::{
    BinaryOperator, Expression, IdentifierReference, IfStatement, Statement, TSLiteral, TSType,
    TSTypeName,
};
use oxc_semantic::{NodeId, Semantic};
use oxc_span::{GetSpan, Span};
use std::sync::Arc;

/// True when `expr` is, or chains off, a bare `expect(...)` call — covering
/// `expect(a).toBe(b)`, `expect(a).to.equal(b)` (chai), `await expect(p)...`
/// (chai-as-promised) and bare `expect(a)`. Walks the call/member chain down
/// to its root identifier.
fn is_expect_chain(expr: &Expression) -> bool {
    match expr.without_parentheses() {
        Expression::Identifier(id) => id.name.as_str() == "expect",
        Expression::CallExpression(call) => is_expect_chain(&call.callee),
        Expression::StaticMemberExpression(m) => is_expect_chain(&m.object),
        Expression::ComputedMemberExpression(m) => is_expect_chain(&m.object),
        Expression::AwaitExpression(a) => is_expect_chain(&a.argument),
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

/// `Some(true)` when every arm of the chain asserts and the chain ends in a
/// bare `else`; `Some(false)` when every arm asserts but the chain ends in an
/// `else if`; `None` when some arm does not assert.
fn chain_arms_assert(if_stmt: &IfStatement) -> Option<bool> {
    if !branch_asserts(&if_stmt.consequent) {
        return None;
    }
    match &if_stmt.alternate {
        None => Some(false),
        Some(Statement::IfStatement(else_if)) => chain_arms_assert(else_if),
        Some(alternate) => branch_asserts(alternate).then_some(true),
    }
}

/// True when the chain always executes one of its arms' assertions, so no run
/// of the test can silently skip them: every arm asserts, and either a bare
/// `else` catches the values the earlier arms miss or the arms compare one
/// discriminant against every member of its declared literal union.
fn chain_always_asserts(if_stmt: &IfStatement, semantic: &Semantic) -> bool {
    match chain_arms_assert(if_stmt) {
        None => false,
        Some(true) => true,
        Some(false) => chain_covers_declared_union(if_stmt, semantic),
    }
}

/// True when every arm compares the same identifier against a distinct string
/// literal and those literals cover every member of the union type the
/// identifier is declared with — one arm then always runs, `else` or not.
fn chain_covers_declared_union(if_stmt: &IfStatement, semantic: &Semantic) -> bool {
    let mut subject: Option<&IdentifierReference> = None;
    let mut covered: Vec<&str> = Vec::new();
    let mut arm = if_stmt;
    loop {
        let Some((ident, literal)) = identifier_equals_literal(&arm.test) else {
            return false;
        };
        if subject.is_some_and(|first| first.name != ident.name) {
            return false;
        }
        subject = subject.or(Some(ident));
        covered.push(literal);
        match &arm.alternate {
            Some(Statement::IfStatement(next)) => arm = next,
            _ => break,
        }
    }
    let Some(members) = subject.and_then(|ident| declared_literal_union(ident, semantic)) else {
        return false;
    };
    !members.is_empty() && members.iter().all(|member| covered.contains(member))
}

/// The identifier and string literal of an `x === 'lit'` test, either way
/// round. `None` for any other comparison.
fn identifier_equals_literal<'a>(test: &'a Expression<'a>) -> Option<(&'a IdentifierReference<'a>, &'a str)> {
    let Expression::BinaryExpression(bin) = test.without_parentheses() else {
        return None;
    };
    if !matches!(
        bin.operator,
        BinaryOperator::StrictEquality | BinaryOperator::Equality
    ) {
        return None;
    }
    match (
        bin.left.without_parentheses(),
        bin.right.without_parentheses(),
    ) {
        (Expression::Identifier(id), Expression::StringLiteral(lit))
        | (Expression::StringLiteral(lit), Expression::Identifier(id)) => {
            Some((id, lit.value.as_str()))
        }
        _ => None,
    }
}

/// The string-literal members of the union type `ident` is declared with,
/// resolved through at most one type-alias hop. `None` when the declaration is
/// not in this file or its type is not a union of string literals.
fn declared_literal_union<'a>(
    ident: &IdentifierReference,
    semantic: &'a Semantic<'a>,
) -> Option<Vec<&'a str>> {
    let scoping = semantic.scoping();
    let symbol_id = ident
        .reference_id
        .get()
        .and_then(|ref_id| scoping.get_reference(ref_id).symbol_id())?;
    let annotation = match semantic.nodes().kind(scoping.symbol_declaration(symbol_id)) {
        AstKind::VariableDeclarator(decl) => decl.type_annotation.as_ref()?,
        AstKind::FormalParameter(param) => param.type_annotation.as_ref()?,
        _ => return None,
    };
    literal_union_members(&annotation.type_annotation, semantic, true)
}

fn literal_union_members<'a>(
    ts_type: &TSType<'a>,
    semantic: &'a Semantic<'a>,
    follow_alias: bool,
) -> Option<Vec<&'a str>> {
    match ts_type {
        TSType::TSUnionType(union) => union
            .types
            .iter()
            .map(|member| match member {
                TSType::TSLiteralType(lit) => match &lit.literal {
                    TSLiteral::StringLiteral(s) => Some(s.value.as_str()),
                    _ => None,
                },
                _ => None,
            })
            .collect(),
        TSType::TSTypeReference(reference) if follow_alias => {
            let TSTypeName::IdentifierReference(name) = &reference.type_name else {
                return None;
            };
            let scoping = semantic.scoping();
            let symbol_id = name
                .reference_id
                .get()
                .and_then(|ref_id| scoping.get_reference(ref_id).symbol_id())?;
            let AstKind::TSTypeAliasDeclaration(alias) =
                semantic.nodes().kind(scoping.symbol_declaration(symbol_id))
            else {
                return None;
            };
            literal_union_members(&alias.type_annotation, semantic, false)
        }
        _ => None,
    }
}

/// True when a statement beside `guarded` in the same list asserts: whenever
/// the `if` chain is reached that assertion is reached too, so the test cannot
/// pass having checked nothing whichever way the branch goes.
fn asserts_beside(stmts: &[Statement], guarded: Span) -> bool {
    stmts
        .iter()
        .any(|stmt| !span_contains(stmt.span(), guarded) && statement_asserts(stmt))
}

fn span_contains(outer: Span, inner: Span) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

/// True when `if_stmt` is the `else if` arm of `parent`.
fn is_else_if(parent: AstKind, if_stmt: &IfStatement) -> bool {
    matches!(parent, AstKind::IfStatement(head)
        if head.alternate.as_ref().is_some_and(|arm| arm.span() == if_stmt.span))
}

/// True when the `if` cannot leave the test without an assertion: it narrows a
/// type rather than choosing a behaviour, or its chain always asserts.
fn if_is_exempt<'a>(
    if_stmt: &'a IfStatement<'a>,
    from: NodeId,
    semantic: &'a Semantic<'a>,
    source: &str,
    scope: &mut Option<Vec<&'a Statement<'a>>>,
) -> bool {
    if is_type_narrowing(&if_stmt.test) || chain_always_asserts(if_stmt, semantic) {
        return true;
    }
    let scope = scope.get_or_insert_with(|| test_scope_statements(from, semantic));
    is_narrowing_guard(if_stmt, scope, source)
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

        // Walk ancestors up to the enclosing `it()`/`test()` call, tracking the
        // outermost `if` that can silently skip this assertion. An `if` reached
        // after the test call wraps the registration itself (dialect filtering
        // around `describe()`) — test selection, not a conditional assertion.
        let nodes = semantic.nodes();
        let mut chain: Option<&IfStatement> = None;
        let mut in_test = false;
        let mut scope: Option<Vec<&Statement>> = None;
        let mut cur_id = nodes.parent_id(node.id());
        loop {
            if cur_id == node.id() || cur_id == nodes.parent_id(cur_id) {
                break;
            }
            match nodes.kind(cur_id) {
                AstKind::IfStatement(if_stmt) => {
                    let test_span = if_stmt.test.span();
                    let in_condition = call.span.start >= test_span.start
                        && call.span.start < test_span.end;
                    // An `else if` is an arm of its parent chain, which is
                    // judged as a whole when the walk reaches its head.
                    if !in_condition
                        && !is_else_if(nodes.kind(nodes.parent_id(cur_id)), if_stmt)
                        && !if_is_exempt(if_stmt, node.id(), semantic, ctx.source, &mut scope)
                    {
                        chain = Some(if_stmt);
                    }
                }
                AstKind::BlockStatement(block) => {
                    if let Some(guarded) = chain
                        && asserts_beside(&block.body, guarded.span)
                    {
                        chain = None;
                    }
                }
                AstKind::FunctionBody(body) => {
                    if let Some(guarded) = chain
                        && asserts_beside(&body.statements, guarded.span)
                    {
                        chain = None;
                    }
                }
                AstKind::CallExpression(ancestor_call) => {
                    if let Expression::Identifier(id) = &ancestor_call.callee
                        && matches!(id.name.as_str(), "test" | "it")
                    {
                        in_test = true;
                        break;
                    }
                }
                _ => {}
            }
            cur_id = nodes.parent_id(cur_id);
        }

        if !in_test {
            return;
        }
        let Some(chain) = chain else { return };
        let (line, column) = byte_offset_to_line_col(ctx.source, chain.span.start as usize);
        // The hazard belongs to the chain, not to each `expect` it holds: the
        // first assertion reached reports it for all of them.
        if diagnostics
            .iter()
            .any(|d| d.line == line && d.column == column)
        {
            return;
        }
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Assertions inside this if-branch are silently skipped when the branch is not taken \u{2014} make them unconditional.".into(),
            severity: super::META.severity,
            span: None,
        });
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

    // Regression for #293: an `if` whose condition is guaranteed by an
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

    // The subject is a test that can pass having checked nothing. Any
    // assertion that always runs beside the `if` — even one whose matcher is
    // negated — takes the test out of that class.
    #[test]
    fn allows_conditional_assertion_beside_a_negated_one() {
        let src = "test('a', () => {\n\
                     expect(cond).not.toBe(true);\n\
                     if (cond) { expect(value).toBe(1); }\n\
                   });";
        assert!(run(src).is_empty(), "{:?}", run(src));
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
    // assert leaves the earlier assertions conditional. The chain is one
    // decision, so it reports once.
    #[test]
    fn flags_if_else_if_chain_with_non_asserting_final_arm() {
        let src = "it('a', () => {\n\
                     if (c) { expect(a).toBe(1); }\n\
                     else if (d) { expect(b).toBe(2); }\n\
                     else { log('skip'); }\n\
                   });";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
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

    // Regression for #8302 shape A: an exhaustive dispatch over a declared
    // string-literal union always runs one arm, whether the last arm is written
    // `else` or `else if` — the `else if` form must not cost four diagnostics
    // where the `else` form costs none.
    #[test]
    fn allows_else_if_chain_covering_the_declared_union() {
        let src = "type Spec = 'postgres' | 'mysql' | 'mssql' | 'sqlite';\n\
                   declare const spec: Spec;\n\
                   it('a', async () => {\n\
                     const schemas = await getSchemas();\n\
                     if (spec === 'postgres') { expect(schemas).toEqual(['public']); }\n\
                     else if (spec === 'mysql') { expect(schemas).toEqual(['mysql']); }\n\
                     else if (spec === 'mssql') { expect(schemas).toEqual(['dbo']); }\n\
                     else if (spec === 'sqlite') { expect(schemas).toEqual([]); }\n\
                   });";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // A chain covering three of the union's four members can still run without
    // asserting — it stays flagged, once for the whole chain.
    #[test]
    fn flags_else_if_chain_missing_a_union_member() {
        let src = "type Spec = 'postgres' | 'mysql' | 'mssql' | 'sqlite';\n\
                   declare const spec: Spec;\n\
                   it('a', async () => {\n\
                     const schemas = await getSchemas();\n\
                     if (spec === 'postgres') { expect(schemas).toEqual(['public']); }\n\
                     else if (spec === 'mysql') { expect(schemas).toEqual(['mysql']); }\n\
                     else if (spec === 'mssql') { expect(schemas).toEqual(['dbo']); }\n\
                   });";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Regression for #8302 shape C: a branch-specific supplement to an
    // assertion that always runs cannot make the test vacuous.
    #[test]
    fn allows_conditional_supplement_beside_an_unconditional_assertion() {
        let src = "it('c', async () => {\n\
                     const n = await run();\n\
                     if (spec === 'mssql') { expect(n).toBe(1); }\n\
                     expect(n).toBeGreaterThan(0);\n\
                   });";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // The exemption depends on the unconditional assertion, not on the shape of
    // the `if`: drop it and the same `if` is flagged again.
    #[test]
    fn flags_conditional_assertion_without_an_unconditional_one() {
        let src = "it('d', async () => {\n\
                     const n = await run();\n\
                     if (spec === 'mssql') { expect(n).toBe(1); }\n\
                   });";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // `await expect(...)` is the chai-as-promised form of an unconditional
    // assertion.
    #[test]
    fn allows_conditional_supplement_beside_an_awaited_assertion() {
        let src = "it('c', async () => {\n\
                     const n = await run();\n\
                     await expect(p).resolves.toBe(1);\n\
                     if (spec === 'mssql') { expect(n).toBe(1); }\n\
                   });";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // A loop may run zero times, so an assertion inside one does not always
    // run — the conditional assertion stays flagged.
    #[test]
    fn flags_conditional_assertion_when_the_other_assertion_is_in_a_loop() {
        let src = "it('c', async () => {\n\
                     const n = await run();\n\
                     if (spec === 'mssql') { expect(n).toBe(1); }\n\
                     for (const x of xs) { expect(x).toBe(0); }\n\
                   });";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // The unconditional assertion exempts the `if` beside it, not an `if` that
    // encloses it: when the outer branch is not taken nothing asserts.
    #[test]
    fn flags_outer_if_enclosing_the_unconditional_assertion() {
        let src = "it('x', () => {\n\
                     if (outer) {\n\
                       expect(a).toBe(1);\n\
                       if (inner) { expect(b).toBe(2); }\n\
                     }\n\
                   });";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // A chain holding several assertions is one decision — one diagnostic.
    #[test]
    fn reports_a_chain_once() {
        let src = "it('a', () => {\n\
                     if (c) { expect(a).toBe(1); expect(b).toBe(2); expect(d).toBe(3); }\n\
                   });";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Regression for #8213: the discriminant is fixed by an object-literal
    // assertion in a sibling `test()` of the same `describe` — the `if` only
    // narrows the union so `dataset.value` is callable.
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
}
