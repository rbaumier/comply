//! ts-prefer-switch-on-discriminant oxc backend — flag the two shapes that
//! narrow a tagged union without a compiler-checked exhaustiveness verdict.
//!
//! Detection is syntactic: a literal key from `discriminant_names` on the left
//! of `in`, and an if/else chain whose every arm compares the same
//! `<object>.<discriminant name>` against a case constant.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryExpression, BinaryOperator, Expression, IfStatement, Statement};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BinaryExpression, AstType::IfStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::BinaryExpression(bin) => report_in_narrowing(bin, ctx, diagnostics),
            AstKind::IfStatement(if_stmt) => {
                // Only chain roots: an `else if` is reached again as the
                // alternate of its parent, and the parent counts the whole chain.
                let parent = semantic.nodes().parent_node(node.id());
                if matches!(parent.kind(), AstKind::IfStatement(_)) {
                    return;
                }
                report_chain(if_stmt, ctx, diagnostics);
            }
            _ => {}
        }
    }
}

fn report_in_narrowing(bin: &BinaryExpression, ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>) {
    let Some(key) = in_narrowing_key(bin) else {
        return;
    };
    if !discriminant_names(ctx).iter().any(|name| name == key) {
        return;
    }
    let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: format!(
            "`\"{key}\" in …` narrows a union by property presence — switch on the \
             `{key}` discriminant and close the switch with `assertNever`, so the \
             compiler reports a variant nobody handled."
        ),
        severity: Severity::Error,
        span: None,
    });
}

fn report_chain(if_stmt: &IfStatement, ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>) {
    let names = discriminant_names(ctx);
    let Some((arms, discriminant)) = chained_discriminant(if_stmt, ctx.source, &names) else {
        return;
    };
    if arms < ctx.config.threshold(super::META.id, "min_branches", ctx.lang) {
        return;
    }
    // One shape, one diagnostic: `prefer-switch-over-chained-if` owns the
    // long-chain span, so this rule stops where that one starts — unless the
    // project turned it off, which would leave the chain unreported.
    let long_chain = ctx
        .config
        .threshold("prefer-switch-over-chained-if", "min_arms", ctx.lang);
    if arms >= long_chain && ctx.config.is_rule_enabled("prefer-switch-over-chained-if", ctx.path) {
        return;
    }
    let (line, column) = byte_offset_to_line_col(ctx.source, if_stmt.span.start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: format!(
            "`{discriminant}` is tested {arms} times in one if/else chain — switch on \
             it and close the switch with `assertNever`, so the compiler reports a \
             variant nobody handled."
        ),
        severity: Severity::Error,
        span: None,
    });
}

fn discriminant_names(ctx: &CheckCtx) -> Vec<String> {
    ctx.config
        .string_list(super::META.id, "discriminant_names", ctx.lang)
}

/// The literal key a `"kind" in x` test probes. A computed key (`key in record`)
/// reads a dictionary rather than a union tag, so it yields `None`.
fn in_narrowing_key<'a>(bin: &'a BinaryExpression<'a>) -> Option<&'a str> {
    if bin.operator != BinaryOperator::In {
        return None;
    }
    let Expression::StringLiteral(key) = bin.left.without_parentheses() else {
        return None;
    };
    Some(key.value.as_str())
}

/// Arm count and discriminant source text of an if/else-if chain whose every
/// arm compares the same discriminant property against a case constant. A chain
/// that mixes discriminants, or that tests anything else, yields `None`.
fn chained_discriminant<'s>(
    stmt: &IfStatement,
    source: &'s str,
    names: &[String],
) -> Option<(usize, &'s str)> {
    let mut shared: Option<&'s str> = None;
    let mut arms = 0;
    let mut current = stmt;
    loop {
        let discriminant = arm_discriminant(&current.test, source, names)?;
        match shared {
            None => shared = Some(discriminant),
            Some(existing) if existing != discriminant => return None,
            Some(_) => {}
        }
        arms += 1;
        match &current.alternate {
            Some(Statement::IfStatement(next)) => current = next,
            _ => break,
        }
    }
    shared.map(|discriminant| (arms, discriminant))
}

/// Source text of the `<object>.<name>` property a chain arm compares against a
/// case constant, when `name` is one of `names`.
fn arm_discriminant<'s>(test: &Expression, source: &'s str, names: &[String]) -> Option<&'s str> {
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
    // Exactly one side is the constant; the other is the discriminant read.
    let discriminant = match (is_case_constant(left), is_case_constant(right)) {
        (true, false) => right,
        (false, true) => left,
        _ => return None,
    };
    let Expression::StaticMemberExpression(member) = discriminant else {
        return None;
    };
    if !names
        .iter()
        .any(|name| name == member.property.name.as_str())
    {
        return None;
    }
    Some(&source[member.span.start as usize..member.span.end as usize])
}

/// A tag a discriminated union labels its variants with.
fn is_case_constant(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::StringLiteral(_) | Expression::NumericLiteral(_)
    )
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

    // --- `in` narrowing on a discriminant name ---

    #[test]
    fn flags_in_on_kind() {
        assert_eq!(run(r#"function f(x: Change) { if ("kind" in x) return 1; }"#).len(), 1);
    }

    #[test]
    fn flags_in_on_type() {
        assert_eq!(run(r#"const isNode = "type" in value;"#).len(), 1);
    }

    #[test]
    fn flags_negated_in() {
        assert_eq!(run(r#"if (!("status" in event)) { return; }"#).len(), 1);
    }

    #[test]
    fn flags_in_inside_a_ternary() {
        assert_eq!(run(r#"const label = "variant" in item ? a : b;"#).len(), 1);
    }

    #[test]
    fn ignores_in_with_a_computed_key() {
        // A dictionary lookup: the key is a variable, so nothing names a union tag.
        assert!(run("if (key in record) { return record[key]; }").is_empty());
    }

    #[test]
    fn ignores_in_with_a_key_outside_the_list() {
        assert!(run(r#"if ("payload" in message) { return 1; }"#).is_empty());
    }

    #[test]
    fn ignores_for_in_loop() {
        assert!(run("for (const kind in registry) { use(kind); }").is_empty());
    }

    // --- if/else chain on one discriminant property ---

    #[test]
    fn flags_two_arm_chain() {
        let src = "
function f(change: Change) {
    if (change.kind === 'add') return 1;
    else if (change.kind === 'remove') return 2;
}
";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_three_arm_chain_with_trailing_else() {
        let src = "
function f(change: Change) {
    if (change.kind === 'add') return 1;
    else if (change.kind === 'remove') return 2;
    else if (change.kind === 'move') return 3;
    else return 0;
}
";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_numeric_tags() {
        let src = "
function f(frame: Frame) {
    if (frame.variant === 0) return 1;
    else if (frame.variant === 1) return 2;
}
";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_constant_on_the_left() {
        let src = "
function f(job: Job) {
    if ('queued' === job.status) return 1;
    else if ('done' === job.status) return 2;
}
";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_nested_object_discriminant() {
        let src = "
function f(envelope: Envelope) {
    if (envelope.body.type === 'text') return 1;
    else if (envelope.body.type === 'image') return 2;
}
";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn reports_one_diagnostic_per_chain() {
        let src = "
function f(change: Change) {
    if (change.kind === 'add') return 1;
    else if (change.kind === 'remove') return 2;
    else if (change.kind === 'move') return 3;
}
";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_different_properties() {
        let src = "
function f(item: Item) {
    if (item.kind === 'a') return 1;
    else if (item.type === 'b') return 2;
}
";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_different_objects() {
        let src = "
function f(left: Item, right: Item) {
    if (left.kind === 'a') return 1;
    else if (right.kind === 'b') return 2;
}
";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_property_outside_the_list() {
        let src = "
function f(user: User) {
    if (user.role === 'admin') return 1;
    else if (user.role === 'guest') return 2;
}
";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_bare_identifier_discriminant() {
        // `prefer-switch-over-chained-if` owns a chain with no property read.
        let src = "
function f(kind: string) {
    if (kind === 'a') return 1;
    else if (kind === 'b') return 2;
}
";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_single_arm_if() {
        assert!(run("function f(c: Change) { if (c.kind === 'add') return 1; }").is_empty());
    }

    #[test]
    fn ignores_inequality_chain() {
        let src = "
function f(change: Change) {
    if (change.kind !== 'add') return 1;
    else if (change.kind !== 'remove') return 2;
}
";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_chain_with_a_predicate_arm() {
        let src = "
function f(change: Change) {
    if (change.kind === 'add') return 1;
    else if (isRemoval(change)) return 2;
}
";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_sibling_ifs() {
        // Two separate statements are not one chain — nothing says they read
        // the same value at the same moment.
        let src = "
function f(change: Change) {
    if (change.kind === 'add') { add(); }
    if (change.kind === 'remove') { remove(); }
}
";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_long_chain_owned_by_prefer_switch_over_chained_if() {
        let src = "
function f(change: Change) {
    if (change.kind === 'add') return 1;
    else if (change.kind === 'remove') return 2;
    else if (change.kind === 'move') return 3;
    else if (change.kind === 'rename') return 4;
}
";
        assert!(run(src).is_empty());
    }

    // --- the shape the rule asks for ---

    #[test]
    fn ignores_existing_switch() {
        let src = "
function f(change: Change) {
    switch (change.kind) {
        case 'add':
            return 1;
        case 'remove':
            return 2;
        default:
            return assertNever(change);
    }
}
";
        assert!(run(src).is_empty());
    }
}
