//! no-typeof-prefer-schema oxc backend — flag `&&` chains that validate an
//! object's shape with `typeof` (an object-ness gate plus a property check, or
//! two property checks on the same object) **when the validated object is the
//! direct local result of a deserialization call** (`JSON.parse(...)` /
//! `<x>.json(...)`), i.e. raw boundary data whose shape belongs in a schema.
//! A lone `typeof x === 'string'` narrowing is left to `prefer-type-guard`;
//! `typeof === 'function'` / `'undefined'` and environment globals are not
//! shape validation; type-dispatch over a parameter or any value without a
//! visible local parse origin is legitimate `any`/`unknown` narrowing.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, is_inside_type_predicate_fn};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    BinaryExpression, BinaryOperator, Expression, IdentifierReference, LogicalOperator,
    UnaryOperator,
};
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

/// A classified `typeof` leaf inside an `&&` chain, keyed by the base object,
/// carrying a reference to that object so its binding origin can be resolved.
enum Leaf<'a> {
    /// `typeof base === 'object'` — the object-ness gate of a shape check.
    ObjectGate(&'a str, &'a IdentifierReference<'a>),
    /// `typeof base.prop === '<primitive>'` — one validated property.
    PropCheck(&'a str, &'a IdentifierReference<'a>),
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

        // A function whose return type is a type predicate (`value is T` or
        // `asserts value is T`) IS a user-defined type guard: chained `typeof`
        // checks are the narrowing primitive it implements, so "prefer a schema"
        // is circular — the guard would still need the same `typeof` checks.
        if is_inside_type_predicate_fn(node.id(), semantic) {
            return;
        }

        let mut leaves = Vec::new();
        collect_leaves(&logical.left, &mut leaves);
        collect_leaves(&logical.right, &mut leaves);

        let mut object_gates: FxHashSet<&str> = FxHashSet::default();
        let mut prop_counts: FxHashMap<&str, usize> = FxHashMap::default();
        let mut base_idents: FxHashMap<&str, &IdentifierReference> = FxHashMap::default();
        for leaf in &leaves {
            let Expression::BinaryExpression(bin) = leaf else { continue };
            match classify(bin) {
                Some(Leaf::ObjectGate(base, ident)) => {
                    object_gates.insert(base);
                    base_idents.entry(base).or_insert(ident);
                }
                Some(Leaf::PropCheck(base, ident)) => {
                    *prop_counts.entry(base).or_default() += 1;
                    base_idents.entry(base).or_insert(ident);
                }
                None => {}
            }
        }

        // Shape validation = two `typeof base.prop` checks on the same object,
        // or an object-ness gate plus at least one property check — but only
        // worth flagging when the object is the direct local result of a
        // deserialization call (raw boundary data a schema should parse). Hand-
        // rolled type-dispatch over a parameter or any value without a visible
        // local parse origin is legitimate `any`/`unknown` narrowing, not a
        // missed schema, so it is left alone.
        let fires = prop_counts.iter().any(|(base, &count)| {
            let shape = count >= 2 || (count >= 1 && object_gates.contains(base));
            shape
                && base_idents
                    .get(base)
                    .is_some_and(|&ident| binding_init_is_deserialization(ident, semantic))
        });
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
            Some(Leaf::ObjectGate(id.name.as_str(), id))
        }
        // `typeof base.prop === '<primitive>'` — one validated property.
        Expression::StaticMemberExpression(m) => {
            let Expression::Identifier(obj) = &m.object else { return None };
            if ENV_GLOBALS.contains(&obj.name.as_str()) {
                return None;
            }
            Some(Leaf::PropCheck(obj.name.as_str(), obj))
        }
        _ => None,
    }
}

/// True when `ident` resolves to a local variable binding whose initializer is a
/// deserialization call — `JSON.parse(...)` or `<x>.json(...)`, optionally
/// awaited. Such a binding holds raw, untyped boundary data whose shape belongs
/// in a schema. A parameter, loop variable, imported binding, or any value
/// without a visible local parse origin resolves to a non-`VariableDeclarator`
/// declaration (or a non-deserialization initializer) and is left alone — its
/// `typeof` dispatch is legitimate `any`/`unknown` narrowing.
fn binding_init_is_deserialization(
    ident: &IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some(ref_id) = ident.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        if let AstKind::VariableDeclarator(decl) = kind {
            return decl.init.as_ref().is_some_and(is_deserialization_expr);
        }
    }
    false
}

/// True when `expr` is a call that deserializes raw input into an untyped value:
/// `JSON.parse(...)` or `<x>.json(...)` (e.g. `await response.json()`).
fn is_deserialization_expr(expr: &Expression) -> bool {
    let expr = match expr {
        Expression::AwaitExpression(await_expr) => &await_expr.argument,
        other => other,
    };
    let Expression::CallExpression(call) = expr else { return false };
    let Expression::StaticMemberExpression(member) = &call.callee else { return false };
    let prop = member.property.name.as_str();
    // `<x>.json(...)` — HTTP response / body deserialization.
    if prop == "json" {
        return true;
    }
    // `JSON.parse(...)`.
    prop == "parse"
        && matches!(&member.object, Expression::Identifier(obj) if obj.name.as_str() == "JSON")
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
    fn flags_two_property_checks_on_parsed_value() {
        let d = run_on(
            "const x = JSON.parse(input);\n\
             if (typeof x.name === 'string' && typeof x.age === 'number') {}",
        );
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn flags_object_gate_plus_property_on_parsed_value() {
        let d = run_on(
            "const x = JSON.parse(input);\n\
             if (typeof x === 'object' && x !== null && typeof x.name === 'string') {}",
        );
        assert_eq!(d.len(), 1, "{d:?}");
    }

    // The hand-rolled shape guard the rule is built for, over freshly-parsed
    // boundary data (`await response.json()`).
    #[test]
    fn flags_handrolled_shape_guard_on_awaited_json() {
        let d = run_on(
            "async function load(res) {\n  const data = await res.json();\n  \
             return typeof data === 'object' && data !== null \
             && typeof data.name === 'string' && typeof data.age === 'number';\n}",
        );
        assert_eq!(d.len(), 1, "{d:?}");
    }

    // Long chain nests as ((a && b) && c) — must flag exactly once.
    #[test]
    fn flags_long_chain_once() {
        let d = run_on(
            "const x = JSON.parse(s);\n\
             if (typeof x.a === 'string' && typeof x.b === 'number' && typeof x.c === 'boolean') {}",
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

    // Issue #5280: a `value is T` type guard implements narrowing with `typeof`;
    // suggesting a schema is circular. The `is_inside_type_predicate_fn` exemption
    // covers this independently of provenance (here `x` is also a parameter, so
    // the provenance gate exempts it too — see `allows_shape_check_on_parameter`).
    #[test]
    fn allows_type_predicate_guard() {
        let d = run_on(
            "function isUser(x: unknown): x is User {\n  return typeof x === 'object' && x !== null \
             && typeof x.name === 'string' && typeof x.age === 'number';\n}",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    // Arrow type guards are equally narrowing primitives.
    #[test]
    fn allows_type_predicate_arrow_guard() {
        let d = run_on(
            "const isUser = (x: unknown): x is User =>\n  typeof x === 'object' && x !== null \
             && typeof x.name === 'string' && typeof x.age === 'number';",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    // `asserts x is T` assertion signatures are also type-narrowing primitives.
    #[test]
    fn allows_assertion_signature_guard() {
        let d = run_on(
            "function assertUser(x: unknown): asserts x is User {\n  if (!(typeof x === 'object' \
             && x !== null && typeof x.name === 'string' && typeof x.age === 'number')) \
             throw new Error('bad');\n}",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    // A `typeof` shape check nested inside a `.every` callback of a type guard is
    // still the guard's narrowing logic — the issue #5280 reduced shape.
    #[test]
    fn allows_nested_shape_check_in_type_guard() {
        let d = run_on(
            "function isDocs(value: unknown): value is Document {\n  return Array.isArray(value) \
             && value.every((v) => typeof v === 'object' && v !== null \
             && typeof v.description === 'string' && typeof v.schema === 'boolean');\n}",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    // An ordinary function (no type-predicate return type) validating a value it
    // just parsed still flags.
    #[test]
    fn flags_shape_check_on_parsed_value_in_plain_function() {
        let d = run_on(
            "function check(input: string): boolean {\n  const x = JSON.parse(input);\n  \
             return typeof x === 'object' && x !== null \
             && typeof x.name === 'string' && typeof x.age === 'number';\n}",
        );
        assert_eq!(d.len(), 1, "{d:?}");
    }

    // A shape guard over a function PARAMETER is legitimate `unknown`/`any`
    // type-dispatch: the value has no visible local parse origin, so the rule
    // cannot know it is deserialized boundary data and must not push it to a
    // schema.
    #[test]
    fn allows_shape_check_on_parameter() {
        let d = run_on(
            "function check(x) {\n  return typeof x === 'object' && x !== null \
             && typeof x.name === 'string' && typeof x.age === 'number';\n}",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    // Issue #6097: quicktype's JSON-Schema walker validates a `schema` parameter
    // (recursively-passed boundary data, not a fresh local parse) with `typeof`.
    // quicktype IS the schema layer and cannot route this through zod; with no
    // local deserialization origin the guard is left alone.
    #[test]
    fn allows_shape_check_on_schema_parameter() {
        let d = run_on(
            "function nameFromSchema(schema) {\n  \
             if (typeof schema === 'object' && typeof schema.title === 'string') {\n    \
             return schema.title;\n  }\n}",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    // Issue #6097: a `for...of` loop variable over already-deserialized data is a
    // boundary value with no local parse origin — not flagged.
    #[test]
    fn allows_shape_check_on_loop_variable() {
        let d = run_on(
            "function f(c) {\n  for (const r of c.response) {\n    \
             if (typeof r === 'object' && typeof r.body === 'string') {}\n  }\n}",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    // A `const` bound to a non-deserialization call (an ordinary domain object)
    // is already typed — its `typeof` checks are not missed schema validation.
    #[test]
    fn allows_shape_check_on_non_parse_binding() {
        let d = run_on(
            "const x = getUser();\n\
             if (typeof x.name === 'string' && typeof x.age === 'number') {}",
        );
        assert!(d.is_empty(), "{d:?}");
    }
}
