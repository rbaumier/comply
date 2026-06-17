//! prefer-switch-over-chained-if OXC backend — flag 4+ if/else-if chains whose
//! every arm compares one shared discriminant against a constant (i.e. chains
//! that are genuinely rewritable as a `switch`).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression, Statement, UnaryOperator};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::IfStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::IfStatement(if_stmt) = node.kind() else {
            return;
        };

        // Only count chain roots — skip if this if-statement is an else branch.
        let parent = semantic.nodes().parent_node(node.id());
        if matches!(parent.kind(), AstKind::IfStatement(_)) {
            return;
        }

        let min_arms = ctx
            .config
            .threshold("prefer-switch-over-chained-if", "min_arms", ctx.lang);

        let arms = count_chained_arms(if_stmt);
        if arms < min_arms {
            return;
        }

        // Only flag chains that are genuinely switch-convertible: every arm must
        // compare ONE shared discriminant against a constant. Predicate-dispatch
        // chains (`isFoo(x)` / `isBar(x)`) and mixed-discriminant chains have no
        // scrutinee for `switch (x)`, so emitting here is a false positive.
        if !shares_single_discriminant(if_stmt, ctx.source) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, if_stmt.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "{arms}-branch if/else-if chain — convert to a \
                 `switch` statement. Switch makes the discriminant \
                 obvious and the TypeScript compiler can warn on \
                 missing cases for union-typed values."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn count_chained_arms(stmt: &oxc_ast::ast::IfStatement) -> usize {
    let mut arms = 1;
    let mut current = stmt;
    loop {
        match &current.alternate {
            Some(Statement::IfStatement(next)) => {
                arms += 1;
                current = next;
            }
            _ => break,
        }
    }
    arms
}

/// True when every arm of the chain (the leading `if` and each `else if`)
/// compares the SAME discriminant against a constant, i.e. the chain can be
/// rewritten as `switch (discriminant) { case <const>: ... }`.
fn shares_single_discriminant(stmt: &oxc_ast::ast::IfStatement, source: &str) -> bool {
    let mut shared: Option<&str> = None;
    let mut current = stmt;
    loop {
        let Some(discriminant) = discriminant_text(&current.test, source) else {
            return false;
        };
        match shared {
            None => shared = Some(discriminant),
            Some(existing) if existing != discriminant => return false,
            Some(_) => {}
        }
        match &current.alternate {
            Some(Statement::IfStatement(next)) => current = next,
            _ => break,
        }
    }
    shared.is_some()
}

/// Source text of the discriminant a `test` compares against a constant, or
/// `None` when the test is not an equality-vs-constant shape (a predicate call,
/// `!==`/`!=`, a logical `&&`/`||`, a relational `<`/`>`, etc.).
///
/// Accepted shapes:
/// - `expr === <literal>` / `expr == <literal>` → discriminant is the non-literal side.
/// - `typeof expr === <string-literal>` → discriminant is `expr`.
///   (the literal must sit on the non-`typeof` side of the equality).
fn discriminant_text<'a>(test: &Expression, source: &'a str) -> Option<&'a str> {
    let Expression::BinaryExpression(bin) = test.without_parentheses() else {
        return None;
    };
    if !matches!(
        bin.operator,
        BinaryOperator::Equality | BinaryOperator::StrictEquality
    ) {
        return None;
    }

    let left = bin.left.without_parentheses();
    let right = bin.right.without_parentheses();

    // Exactly one side must be a constant literal; the other is the discriminant.
    let discriminant = match (is_constant_literal(left), is_constant_literal(right)) {
        (true, false) => right,
        (false, true) => left,
        _ => return None,
    };

    // `typeof expr === '...'` compares the type tag: the real discriminant is
    // the operand of `typeof`, captured by its source text so distinct operands
    // (e.g. `typeof a` vs `typeof b`) do not collapse together.
    if let Expression::UnaryExpression(unary) = discriminant
        && unary.operator == UnaryOperator::Typeof
    {
        let arg = unary.argument.without_parentheses();
        return Some(span_text(arg, source));
    }

    Some(span_text(discriminant, source))
}

/// A switch `case` constant: string / number / bigint / null literal.
fn is_constant_literal(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::StringLiteral(_)
            | Expression::NumericLiteral(_)
            | Expression::BigIntLiteral(_)
            | Expression::NullLiteral(_)
    )
}

fn span_text<'a>(expr: &Expression, source: &'a str) -> &'a str {
    let span = expr.span();
    &source[span.start as usize..span.end as usize]
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

    // --- True positives: a single shared discriminant compared `=== <const>`. ---

    #[test]
    fn flags_single_identifier_discriminant() {
        let source = "
function f(k: string) {
    if (k === 'a') return 1;
    else if (k === 'b') return 2;
    else if (k === 'c') return 3;
    else if (k === 'd') return 4;
}
";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_typeof_discriminant() {
        let source = "
function f(x: unknown) {
    if (typeof x === 'string') return 1;
    else if (typeof x === 'number') return 2;
    else if (typeof x === 'boolean') return 3;
    else if (typeof x === 'undefined') return 4;
}
";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_shared_member_discriminant() {
        let source = "
function f(obj: any) {
    if (obj.kind === 'a') return 1;
    else if (obj.kind === 'b') return 2;
    else if (obj.kind === 'c') return 3;
    else if (obj.kind === 'd') return 4;
}
";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_literal_on_the_left() {
        let source = "
function f(k: string) {
    if ('a' === k) return 1;
    else if ('b' === k) return 2;
    else if ('c' === k) return 3;
    else if ('d' === k) return 4;
}
";
        assert_eq!(run_on(source).len(), 1);
    }

    // --- False positives the fix removes: no single switch-convertible discriminant. ---

    #[test]
    fn ignores_predicate_dispatch_chain() {
        // mobx object-api.ts:97 — every arm calls a different boolean predicate
        // on `obj`; there is no scrutinee for `switch (obj)`.
        let source = "
function set(obj: any, key: any, value: any) {
    if (isObservableObject(obj)) {
        obj.set(key, value);
    } else if (isObservableMap(obj)) {
        obj.set(key, value);
    } else if (isObservableSet(obj)) {
        obj.add(key);
    } else if (isObservableArray(obj)) {
        obj.push(value);
    } else {
        die(8);
    }
}
";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn ignores_mixed_predicate_and_inequality_chain() {
        // mobx observablemap.ts:362 — predicates plus a `!==`/`&&` arm.
        let source = "
function merge(other: any) {
    if (isPlainObject(other)) {
        a();
    } else if (Array.isArray(other)) {
        b();
    } else if (isES6Map(other)) {
        c();
    } else if (other !== null && other !== undefined) {
        d();
    }
}
";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn ignores_mixed_discriminant_chain() {
        let source = "
function f(a: number, b: number, c: number, d: number) {
    if (a === 1) return 1;
    else if (b === 2) return 2;
    else if (c === 3) return 3;
    else if (d === 4) return 4;
}
";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn ignores_inequality_chain() {
        let source = "
function f(k: number) {
    if (k !== 1) return 1;
    else if (k !== 2) return 2;
    else if (k !== 3) return 3;
    else if (k !== 4) return 4;
}
";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn ignores_relational_chain() {
        let source = "
function f(k: number) {
    if (k < 1) return 1;
    else if (k < 2) return 2;
    else if (k < 3) return 3;
    else if (k < 4) return 4;
}
";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn ignores_typeof_with_mixed_operands() {
        // `typeof a` vs `typeof b` are distinct discriminants — not convertible.
        let source = "
function f(a: unknown, b: unknown, c: unknown, d: unknown) {
    if (typeof a === 'string') return 1;
    else if (typeof b === 'number') return 2;
    else if (typeof c === 'boolean') return 3;
    else if (typeof d === 'object') return 4;
}
";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn ignores_member_discriminant_mismatch() {
        // `obj.kind` vs `obj.type` differ by source text — not a shared discriminant.
        let source = "
function f(obj: any) {
    if (obj.kind === 'a') return 1;
    else if (obj.type === 'b') return 2;
    else if (obj.kind === 'c') return 3;
    else if (obj.type === 'd') return 4;
}
";
        assert!(run_on(source).is_empty());
    }

    // --- Threshold behaviour preserved. ---

    #[test]
    fn allows_three_arm_chain() {
        let source = "
function f(k: string) {
    if (k === 'a') return 1;
    else if (k === 'b') return 2;
    else if (k === 'c') return 3;
}
";
        assert!(run_on(source).is_empty());
    }
}
