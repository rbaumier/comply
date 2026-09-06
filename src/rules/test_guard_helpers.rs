//! Shared "is this `if` a type guard?" decision for `no-test-logic` and
//! `testing-no-conditional-assertion`.
//!
//! An `if` inside a test body narrows a type instead of choosing between
//! behaviours when its test is a recognised narrowing form, or when an
//! assertion elsewhere in the test scope already fixes the value the branch
//! requires. The lookup spans the whole enclosing `describe` (or module) body,
//! including sibling `test()` bodies: the vitest/jest idiom binds a subject
//! once at `describe` scope, asserts its shape in one test and narrows it in
//! the next, so the assertion that proves the branch lives outside the block.
//!
//! Both rules answer this question about the same `if`; one implementation
//! keeps their verdicts identical.

use crate::rules::backend::AstKind;
use oxc_ast::ast::{
    Argument, BinaryExpression, BinaryOperator, Expression, IfStatement, ObjectPropertyKind,
    PropertyKey, Statement, UnaryOperator,
};
use oxc_semantic::{NodeId, Semantic};
use oxc_span::{GetSpan, Span};

/// Callees whose callback body belongs to the same test scope: assertions
/// inside them are visible to a guard anywhere else in that scope.
const TEST_SCOPE_CALLEES: &[&str] = &[
    "describe",
    "suite",
    "context",
    "test",
    "it",
    "beforeEach",
    "afterEach",
    "beforeAll",
    "afterAll",
    "before",
    "after",
];

/// Callees that open a new test scope root — the outermost body a guard
/// lookup scans.
const SUITE_CALLEES: &[&str] = &["describe", "suite", "context"];

/// True when `expr` narrows a type by construction: `x instanceof Foo`, a
/// nullish comparison, a `Result`-style `isOk()`/`isErr()` probe, or `!` of
/// any of those.
#[must_use]
pub fn is_type_narrowing(expr: &Expression) -> bool {
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
            matches!(unary.operator, UnaryOperator::LogicalNot) && is_type_narrowing(&unary.argument)
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

/// The value an `if` branch requires of its test expression for the branch to
/// be taken. Source text, so an assertion is matched by comparing the text of
/// the expression it asserts on.
enum RequiredValue<'s> {
    /// `if (X)` — the branch runs when `X` is truthy.
    Truthy(&'s str),
    /// `if (!X)` — the branch runs when `X` is falsy.
    Falsy(&'s str),
    /// `if (A === B)` — the branch runs when `A` equals `B`.
    Equal { subject: &'s str, value: &'s str },
}

fn required_value<'s>(test: &Expression, source: &'s str) -> Option<RequiredValue<'s>> {
    match test.without_parentheses() {
        Expression::UnaryExpression(u) if matches!(u.operator, UnaryOperator::LogicalNot) => {
            match required_value(&u.argument, source)? {
                RequiredValue::Truthy(t) => Some(RequiredValue::Falsy(t)),
                RequiredValue::Falsy(t) => Some(RequiredValue::Truthy(t)),
                // `!(a === b)` requires an inequality assertion, which no
                // recognised form expresses.
                RequiredValue::Equal { .. } => None,
            }
        }
        Expression::BinaryExpression(bin)
            if matches!(
                bin.operator,
                BinaryOperator::StrictEquality | BinaryOperator::Equality
            ) =>
        {
            Some(RequiredValue::Equal {
                subject: span_text(bin.left.span(), source),
                value: span_text(bin.right.span(), source),
            })
        }
        other => Some(RequiredValue::Truthy(span_text(other.span(), source))),
    }
}

/// True when the `if` is type-narrowing boilerplate rather than hidden control
/// flow: either its test narrows by construction, or a statement in `scope`
/// asserts the very value the branch requires. Statements nested inside the
/// `if` itself are ignored — an assertion in the guarded branch cannot prove
/// the branch is taken.
#[must_use]
pub fn is_narrowing_guard(if_stmt: &IfStatement, scope: &[&Statement], source: &str) -> bool {
    if is_type_narrowing(&if_stmt.test) {
        return true;
    }
    let Some(required) = required_value(&if_stmt.test, source) else {
        return false;
    };
    scope
        .iter()
        .filter(|stmt| !span_contains(if_stmt.span, stmt.span()))
        .any(|stmt| assertion_fixes(stmt, &required, source))
}

fn span_contains(outer: Span, inner: Span) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

fn span_text(span: Span, source: &str) -> &str {
    source[span.start as usize..span.end as usize].trim()
}

/// A matched `expect(<subject>).<matcher>(<value>)` call: the plain jest/vitest
/// form only, so `expect(x).not.toBe(true)` — which proves the opposite — never
/// matches.
struct ExpectAssertion<'a> {
    subject: &'a Expression<'a>,
    matcher: &'a str,
    value: Option<&'a Expression<'a>>,
}

fn expect_assertion<'a>(stmt: &'a Statement<'a>) -> Option<ExpectAssertion<'a>> {
    let Statement::ExpressionStatement(es) = stmt else {
        return None;
    };
    let Expression::CallExpression(call) = es.expression.without_parentheses() else {
        return None;
    };
    let Expression::StaticMemberExpression(matcher) = &call.callee else {
        return None;
    };
    let Expression::CallExpression(expect_call) = &matcher.object else {
        return None;
    };
    let Expression::Identifier(id) = &expect_call.callee else {
        return None;
    };
    if id.name.as_str() != "expect" {
        return None;
    }
    Some(ExpectAssertion {
        subject: expect_call.arguments.first()?.as_expression()?,
        matcher: matcher.property.name.as_str(),
        value: call.arguments.first().and_then(Argument::as_expression),
    })
}

fn assertion_fixes(stmt: &Statement, required: &RequiredValue, source: &str) -> bool {
    let Some(assertion) = expect_assertion(stmt) else {
        return false;
    };
    let subject = span_text(assertion.subject.span(), source);
    match required {
        RequiredValue::Truthy(target) => fixes_boolean(&assertion, subject, target, true),
        RequiredValue::Falsy(target) => fixes_boolean(&assertion, subject, target, false),
        RequiredValue::Equal { subject: s, value } => {
            if matches!(assertion.matcher, "toBe" | "toEqual" | "toStrictEqual") {
                let asserted = assertion.value.map(|v| span_text(v.span(), source));
                if asserted == Some(*value) && subject == *s
                    || asserted == Some(*s) && subject == *value
                {
                    return true;
                }
            }
            property_value_text(&assertion, subject, s, source) == Some(*value)
        }
    }
}

/// True when the assertion pins `target` to `expected` — either directly
/// (`expect(target).toBe(true)`, `expect(target).toBeFalsy()`) or through an
/// object-literal assertion on the object `target` reads from
/// (`expect(obj).toStrictEqual({ prop: true, … })`).
fn fixes_boolean(assertion: &ExpectAssertion, subject: &str, target: &str, expected: bool) -> bool {
    if subject == target {
        let by_name = if expected { "toBeTruthy" } else { "toBeFalsy" };
        if assertion.matcher == by_name {
            return true;
        }
        if matches!(assertion.matcher, "toBe" | "toEqual" | "toStrictEqual")
            && matches!(assertion.value.map(Expression::without_parentheses),
                Some(Expression::BooleanLiteral(b)) if b.value == expected)
        {
            return true;
        }
    }
    matches!(
        property_value(assertion, subject, target).map(Expression::without_parentheses),
        Some(Expression::BooleanLiteral(b)) if b.value == expected
    )
}

fn property_value_text<'s>(
    assertion: &ExpectAssertion,
    subject: &str,
    target: &str,
    source: &'s str,
) -> Option<&'s str> {
    property_value(assertion, subject, target).map(|v| span_text(v.span(), source))
}

/// The value an object-literal assertion on `subject` gives to the property
/// `target` reads — `expect(obj).toStrictEqual({ typed: true })` answers
/// `true` for the target `obj.typed`.
fn property_value<'a>(
    assertion: &ExpectAssertion<'a>,
    subject: &str,
    target: &str,
) -> Option<&'a Expression<'a>> {
    if !matches!(
        assertion.matcher,
        "toEqual" | "toStrictEqual" | "toMatchObject"
    ) {
        return None;
    }
    let Some(Expression::ObjectExpression(obj)) = assertion.value.map(Expression::without_parentheses)
    else {
        return None;
    };
    obj.properties.iter().find_map(|prop| {
        let ObjectPropertyKind::ObjectProperty(p) = prop else {
            return None;
        };
        let key = match &p.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => return None,
        };
        member_text_matches(target, subject, key).then_some(&p.value)
    })
}

/// True when `target` is exactly the source text `<object>.<key>`.
fn member_text_matches(target: &str, object: &str, key: &str) -> bool {
    target.len() == object.len() + 1 + key.len()
        && target.starts_with(object)
        && target.as_bytes()[object.len()] == b'.'
        && target.ends_with(key)
}

/// Every statement of the test scope enclosing `from` — the nearest
/// `describe()` callback body, or the module body when there is none —
/// including the bodies of the sibling `test()` / `it()` / hook callbacks
/// registered in it.
#[must_use]
pub fn test_scope_statements<'a>(
    from: NodeId,
    semantic: &'a Semantic<'a>,
) -> Vec<&'a Statement<'a>> {
    let nodes = semantic.nodes();
    let mut out = Vec::new();
    let mut cur = from;
    loop {
        match nodes.kind(cur) {
            AstKind::CallExpression(call)
                if callee_root_name(&call.callee).is_some_and(|n| SUITE_CALLEES.contains(&n)) =>
            {
                if let Some(body) = callback_body(&call.arguments) {
                    collect_statements(body, &mut out);
                    return out;
                }
            }
            AstKind::Program(program) => {
                collect_statements(&program.body, &mut out);
                return out;
            }
            _ => {}
        }
        let parent = nodes.parent_id(cur);
        if parent == cur {
            return out;
        }
        cur = parent;
    }
}

/// Base identifier of a callee, seeing through `it.each([…])(…)`, `test.only`
/// and `describe.each` so those register in the scope they belong to.
fn callee_root_name<'a>(callee: &'a Expression<'a>) -> Option<&'a str> {
    match callee.without_parentheses() {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(m) => callee_root_name(&m.object),
        Expression::ComputedMemberExpression(m) => callee_root_name(&m.object),
        Expression::CallExpression(inner) => callee_root_name(&inner.callee),
        Expression::TaggedTemplateExpression(t) => callee_root_name(&t.tag),
        _ => None,
    }
}

/// Statement list of the last function/arrow argument — the callback a test
/// framework entry point runs.
fn callback_body<'a>(args: &'a [Argument<'a>]) -> Option<&'a [Statement<'a>]> {
    args.iter().rev().find_map(|arg| match arg.as_expression()? {
        Expression::ArrowFunctionExpression(arrow) => Some(&arrow.body.statements[..]),
        Expression::FunctionExpression(func) => Some(&func.body.as_ref()?.statements[..]),
        _ => None,
    })
}

/// Flattens every statement reachable inside the test scope, descending into
/// blocks, branches, loops and the callbacks of test framework entry points —
/// but not into unrelated function bodies, whose statements need not run.
fn collect_statements<'a>(stmts: &'a [Statement<'a>], out: &mut Vec<&'a Statement<'a>>) {
    for stmt in stmts {
        out.push(stmt);
        match stmt {
            Statement::BlockStatement(b) => collect_statements(&b.body, out),
            Statement::LabeledStatement(l) => collect_statements(std::slice::from_ref(&l.body), out),
            Statement::IfStatement(s) => {
                collect_statements(std::slice::from_ref(&s.consequent), out);
                if let Some(alternate) = &s.alternate {
                    collect_statements(std::slice::from_ref(alternate), out);
                }
            }
            Statement::ForStatement(s) => collect_statements(std::slice::from_ref(&s.body), out),
            Statement::ForInStatement(s) => collect_statements(std::slice::from_ref(&s.body), out),
            Statement::ForOfStatement(s) => collect_statements(std::slice::from_ref(&s.body), out),
            Statement::WhileStatement(s) => collect_statements(std::slice::from_ref(&s.body), out),
            Statement::DoWhileStatement(s) => collect_statements(std::slice::from_ref(&s.body), out),
            Statement::SwitchStatement(s) => {
                for case in &s.cases {
                    collect_statements(&case.consequent, out);
                }
            }
            Statement::TryStatement(s) => {
                collect_statements(&s.block.body, out);
                if let Some(handler) = &s.handler {
                    collect_statements(&handler.body.body, out);
                }
                if let Some(finalizer) = &s.finalizer {
                    collect_statements(&finalizer.body, out);
                }
            }
            Statement::ExpressionStatement(es) => {
                if let Expression::CallExpression(call) = es.expression.without_parentheses()
                    && callee_root_name(&call.callee)
                        .is_some_and(|n| TEST_SCOPE_CALLEES.contains(&n))
                    && let Some(body) = callback_body(&call.arguments)
                {
                    collect_statements(body, out);
                }
            }
            _ => {}
        }
    }
}
