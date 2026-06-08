//! no-typeof-prefer-schema oxc backend — flag `&&` chains that validate an
//! object's shape with `typeof` (an object-ness gate plus a property check, or
//! two property checks on the same object). A lone `typeof x === 'string'`
//! narrowing is left to `prefer-type-guard`; `typeof === 'function'` /
//! `'undefined'` and environment globals are not shape validation.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryExpression, BinaryOperator, Expression, LogicalOperator, UnaryOperator};
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;

/// `typeof` operands that are environment/feature detection, not shape
/// validation — no schema can replace them.
const ENV_GLOBALS: &[&str] = &[
    "window",
    "document",
    "navigator",
    "location",
    "history",
    "localStorage",
    "sessionStorage",
    "globalThis",
    "self",
    "global",
    "process",
    "performance",
    "crypto",
    "console",
];

/// Shape-validation primitives. `function` and `undefined` are excluded:
/// `=== 'function'` is feature detection and `=== 'undefined'` is presence
/// checking (owned by `no-typeof-undefined`).
const SHAPE_PRIMITIVES: &[&str] = &["string", "number", "boolean", "object", "symbol", "bigint"];

/// A classified `typeof` leaf inside an `&&` chain, keyed by the base object.
enum Leaf<'a> {
    /// `typeof base === 'object'` — the object-ness gate of a shape check.
    ObjectGate(&'a str),
    /// `typeof base.prop === '<primitive>'` — one validated property.
    PropCheck(&'a str),
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::LogicalExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["typeof"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::LogicalExpression(logical) = node.kind() else { return };
        if logical.operator != LogicalOperator::And {
            return;
        }

        // Only act on the outermost `&&` chain: `a && b && c` nests as
        // `((a && b) && c)`, so a sub-chain whose nearest non-paren ancestor is
        // another `&&` would otherwise flag the same chain twice.
        if has_and_ancestor(node, semantic) {
            return;
        }

        let mut leaves = Vec::new();
        collect_leaves(&logical.left, &mut leaves);
        collect_leaves(&logical.right, &mut leaves);

        let mut object_gates: FxHashSet<&str> = FxHashSet::default();
        let mut prop_counts: FxHashMap<&str, usize> = FxHashMap::default();
        for leaf in &leaves {
            let Expression::BinaryExpression(bin) = leaf else { continue };
            match classify(bin) {
                Some(Leaf::ObjectGate(base)) => {
                    object_gates.insert(base);
                }
                Some(Leaf::PropCheck(base)) => {
                    *prop_counts.entry(base).or_default() += 1;
                }
                None => {}
            }
        }

        // Shape validation = two `typeof base.prop` checks on the same object,
        // or an object-ness gate plus at least one property check.
        let fires = prop_counts
            .iter()
            .any(|(base, &count)| count >= 2 || (count >= 1 && object_gates.contains(base)));
        if !fires {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, logical.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Validating an object's shape with chained `typeof` checks is \
                      error-prone — parse it with a schema validator (zod, valibot, …) \
                      instead."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Returns true when the nearest non-parenthesised ancestor is another `&&`
/// expression, i.e. this node is a sub-chain of a larger `&&`.
fn has_and_ancestor<'a>(node: &oxc_semantic::AstNode<'a>, semantic: &'a oxc_semantic::Semantic<'a>) -> bool {
    let nodes = semantic.nodes();
    let mut pid = nodes.parent_id(node.id());
    if pid == node.id() {
        return false;
    }
    loop {
        match nodes.kind(pid) {
            AstKind::ParenthesizedExpression(_) => {
                let next = nodes.parent_id(pid);
                if next == pid {
                    return false;
                }
                pid = next;
            }
            AstKind::LogicalExpression(l) => return l.operator == LogicalOperator::And,
            _ => return false,
        }
    }
}

/// Flatten an `&&` chain into its leaf operands, descending through parentheses
/// and nested `&&` but stopping at anything else.
fn collect_leaves<'a, 'b>(expr: &'b Expression<'a>, leaves: &mut Vec<&'b Expression<'a>>) {
    match expr {
        Expression::ParenthesizedExpression(p) => collect_leaves(&p.expression, leaves),
        Expression::LogicalExpression(l) if l.operator == LogicalOperator::And => {
            collect_leaves(&l.left, leaves);
            collect_leaves(&l.right, leaves);
        }
        _ => leaves.push(expr),
    }
}

/// Classify a binary expression as a shape-validation `typeof` leaf, if it is
/// one. Returns `None` for non-`typeof` comparisons, non-shape primitives
/// (`function`/`undefined`), bare-identifier narrowing (`typeof x === 'string'`),
/// and environment globals.
fn classify<'a>(bin: &'a BinaryExpression<'a>) -> Option<Leaf<'a>> {
    if !matches!(
        bin.operator,
        BinaryOperator::StrictEquality
            | BinaryOperator::StrictInequality
            | BinaryOperator::Equality
            | BinaryOperator::Inequality
    ) {
        return None;
    }

    // One side a `typeof` unary, the other a string literal.
    let (unary, lit_expr) = match (&bin.left, &bin.right) {
        (Expression::UnaryExpression(u), other) | (other, Expression::UnaryExpression(u))
            if u.operator == UnaryOperator::Typeof =>
        {
            (u, other)
        }
        _ => return None,
    };
    let Expression::StringLiteral(lit) = lit_expr else { return None };
    let lit = lit.value.as_str();
    if !SHAPE_PRIMITIVES.contains(&lit) {
        return None;
    }

    match &unary.argument {
        // `typeof x === 'object'` — object-ness gate. A bare identifier with any
        // other primitive is plain narrowing, deliberately not counted.
        Expression::Identifier(id) if lit == "object" && !ENV_GLOBALS.contains(&id.name.as_str()) => {
            Some(Leaf::ObjectGate(id.name.as_str()))
        }
        // `typeof base.prop === '<primitive>'` — one validated property.
        Expression::StaticMemberExpression(m) => {
            let Expression::Identifier(obj) = &m.object else { return None };
            if ENV_GLOBALS.contains(&obj.name.as_str()) {
                return None;
            }
            Some(Leaf::PropCheck(obj.name.as_str()))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_two_property_checks_same_object() {
        let d = run_on("if (typeof x.name === 'string' && typeof x.age === 'number') {}");
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn flags_object_gate_plus_property() {
        let d = run_on("if (typeof x === 'object' && x !== null && typeof x.name === 'string') {}");
        assert_eq!(d.len(), 1, "{d:?}");
    }

    // Regression: the `isUser` shape guard the rule is built for.
    #[test]
    fn flags_handrolled_shape_guard() {
        let d = run_on(
            "function isUser(x) {\n  return typeof x === 'object' && x !== null \
             && typeof x.name === 'string' && typeof x.age === 'number';\n}",
        );
        assert_eq!(d.len(), 1, "{d:?}");
    }

    // Long chain nests as ((a && b) && c) — must flag exactly once.
    #[test]
    fn flags_long_chain_once() {
        let d = run_on(
            "if (typeof x.a === 'string' && typeof x.b === 'number' && typeof x.c === 'boolean') {}",
        );
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn allows_single_primitive_narrowing() {
        let d = run_on("if (typeof x === 'string') {}");
        assert!(d.is_empty(), "{d:?}");
    }

    // `isString` is prefer-type-guard's territory, not ours.
    #[test]
    fn allows_single_property_narrowing() {
        let d = run_on("function isString(x) { return typeof x === 'string'; }");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn allows_two_unrelated_primitive_narrows() {
        let d = run_on("if (typeof a === 'string' && typeof b === 'string') {}");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn allows_function_feature_detection() {
        let d = run_on("if (typeof x.foo === 'function' && typeof x.bar === 'function') {}");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn allows_object_gate_without_property_check() {
        let d = run_on("if (typeof x === 'object' && x !== null) {}");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn allows_env_global_property_checks() {
        let d = run_on(
            "if (typeof window.foo === 'string' && typeof window.bar === 'number') {}",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn allows_two_partial_checks_on_different_objects() {
        let d = run_on("if (typeof x.a === 'string' && typeof y.b === 'number') {}");
        assert!(d.is_empty(), "{d:?}");
    }
}
