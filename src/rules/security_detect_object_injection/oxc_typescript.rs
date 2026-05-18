//! security-detect-object-injection oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, TSType, TSTypeOperatorOperator};
use std::sync::Arc;

const ITER_METHODS: &[&str] = &[
    "map", "forEach", "flatMap", "filter", "find", "reduce", "some", "every",
];

/// Max parent nodes walked from a callback arrow function up to its `CallExpression`:
/// `ArrowFunction` → `Argument` (wrapper) → `Arguments` (list) → `CallExpression` ≤ 3 hops.
const MAX_CALLBACK_ANCESTOR_DEPTH: usize = 3;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ComputedMemberExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ComputedMemberExpression(member) = node.kind() else {
            return;
        };
        // Skip when the key is a literal (string / number / template
        // with no interpolations) — that's a static lookup, not an
        // injection vector.
        match &member.expression {
            Expression::StringLiteral(_) | Expression::NumericLiteral(_) => return,
            Expression::TemplateLiteral(tpl) if tpl.expressions.is_empty() => return,
            _ => {}
        }
        // Skip array literal access `arr[0]` and similar — the rule
        // targets OBJECT injection, not array indexing. Heuristic:
        // if the object is itself an array literal, skip.
        if matches!(&member.object, Expression::ArrayExpression(_)) {
            return;
        }
        // Skip when the key's static type contains a `keyof` operator — bounded by TS at the call site.
        if let Expression::Identifier(id_ref) = &member.expression
            && let Some(ref_id) = id_ref.reference_id.get()
            && let Some(sym_id) = semantic.scoping().get_reference(ref_id).symbol_id()
        {
            let decl_id = semantic.scoping().symbol_declaration(sym_id);
            if param_type_has_keyof(decl_id, semantic)
                || is_iterator_callback_over_keyof_array(decl_id, semantic)
            {
                return;
            }
        }
        // Assignment `obj[key] = …` still flags — write is even riskier
        // than read.
        let (line, column) = byte_offset_to_line_col(ctx.source, member.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Bracket access with a non-literal key — vulnerable to prototype \
                      pollution / data exfiltration if the key comes from untrusted \
                      input. Validate the key against an allowlist first."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True when `decl_id` resolves to a formal parameter whose type
/// annotation contains a `keyof X` operator anywhere in the type tree.
fn param_type_has_keyof(
    decl_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();
    // oxc points `symbol_declaration` at the FormalParameter node for
    // parameter bindings; for nested binding patterns we walk up.
    if let AstKind::FormalParameter(param) = nodes.kind(decl_id) {
        return param
            .type_annotation
            .as_ref()
            .is_some_and(|ann| ts_type_has_keyof(&ann.type_annotation));
    }
    // Variable binding: `const ks: (keyof Foo)[] = …` — check the declarator's
    // explicit type annotation. Untyped declarators fall through to `false`.
    if let AstKind::VariableDeclarator(decl) = nodes.kind(decl_id) {
        return decl
            .type_annotation
            .as_ref()
            .is_some_and(|ann| ts_type_has_keyof(&ann.type_annotation));
    }
    for kind in nodes.ancestor_kinds(decl_id) {
        match kind {
            AstKind::FormalParameter(param) => {
                return param
                    .type_annotation
                    .as_ref()
                    .is_some_and(|ann| ts_type_has_keyof(&ann.type_annotation));
            }
            AstKind::VariableDeclarator(decl) => {
                return decl
                    .type_annotation
                    .as_ref()
                    .is_some_and(|ann| ts_type_has_keyof(&ann.type_annotation));
            }
            AstKind::Function(_)
            | AstKind::ArrowFunctionExpression(_)
            | AstKind::Program(_)
            | AstKind::VariableDeclaration(_) => return false,
            _ => continue,
        }
    }
    false
}

/// True when the binding is the parameter of an iterator-callback (`ITER_METHODS`)
/// whose receiver array's static type contains a `keyof X` operator.
fn is_iterator_callback_over_keyof_array(
    decl_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();
    if !matches!(nodes.kind(decl_id), AstKind::FormalParameter(_)) {
        return false;
    }
    // Walk to the enclosing arrow/function.
    let mut func_id = None;
    for (kind, nid) in nodes.ancestor_kinds(decl_id).zip(nodes.ancestor_ids(decl_id)) {
        match kind {
            AstKind::FormalParameters(_) => continue,
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                func_id = Some(nid);
                break;
            }
            _ => break,
        }
    }
    let Some(func_id) = func_id else { return false };
    // Find the enclosing CallExpression (skipping the Argument wrapper).
    let mut cur = nodes.parent_id(func_id);
    if cur == func_id {
        return false;
    }
    for _ in 0..MAX_CALLBACK_ANCESTOR_DEPTH {
        if let AstKind::CallExpression(call) = nodes.kind(cur) {
            let Expression::StaticMemberExpression(member) = &call.callee else {
                return false;
            };
            if !ITER_METHODS.contains(&member.property.name.as_str()) {
                return false;
            }
            let Expression::Identifier(recv) = &member.object else {
                return false;
            };
            let Some(ref_id) = recv.reference_id.get() else {
                return false;
            };
            let Some(sym_id) = semantic.scoping().get_reference(ref_id).symbol_id() else {
                return false;
            };
            let recv_decl = semantic.scoping().symbol_declaration(sym_id);
            return param_type_has_keyof(recv_decl, semantic);
        }
        let next = nodes.parent_id(cur);
        if next == cur {
            break;
        }
        cur = next;
    }
    false
}

/// Recursively check whether a TS type contains a `keyof X` operator.
/// Covers `keyof T`, `keyof T & string`, `readonly (keyof T)[]`,
/// `Extract<keyof T, string>`, and similar shapes.
fn ts_type_has_keyof(ty: &TSType) -> bool {
    match ty {
        TSType::TSTypeOperatorType(op) => {
            op.operator == TSTypeOperatorOperator::Keyof
                || ts_type_has_keyof(&op.type_annotation)
        }
        TSType::TSIntersectionType(i) => i.types.iter().any(ts_type_has_keyof),
        TSType::TSUnionType(u) => u.types.iter().any(ts_type_has_keyof),
        TSType::TSArrayType(a) => ts_type_has_keyof(&a.element_type),
        TSType::TSParenthesizedType(p) => ts_type_has_keyof(&p.type_annotation),
        TSType::TSTypeReference(r) => r
            .type_arguments
            .as_ref()
            .is_some_and(|args| args.params.iter().any(ts_type_has_keyof)),
        TSType::TSConditionalType(c) => {
            ts_type_has_keyof(&c.check_type)
                || ts_type_has_keyof(&c.extends_type)
                || ts_type_has_keyof(&c.true_type)
                || ts_type_has_keyof(&c.false_type)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_dynamic_bracket_access() {
        let src = r#"function f(obj, key) { return obj[key]; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_static_string_key() {
        let src = r#"function f(obj) { return obj["foo"]; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_array_literal_index() {
        let src = r#"const x = ["a", "b", "c"][i];"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_keyof_typed_parameter() {
        let src = r#"
            function pick<T>(obj: T, key: keyof T) {
                return obj[key];
            }
        "#;
        assert!(run(src).is_empty(), "keyof-typed key should not flag");
    }

    #[test]
    fn allows_keyof_intersection_typed_parameter() {
        let src = r#"
            function pick<T>(obj: T, key: keyof T & string) {
                return obj[key];
            }
        "#;
        assert!(
            run(src).is_empty(),
            "`keyof T & string` typed key should not flag"
        );
    }

    // Regression: `const ks: (keyof Foo)[] = …` — variable with explicit keyof
    // type annotation should not flag when used as bracket-access key.
    #[test]
    fn allows_variable_with_keyof_type_annotation() {
        let src = r#"
            interface Foo { a: number; b: string }
            declare const obj: Foo;
            const ks: (keyof Foo)[] = ["a", "b"];
            ks.map((k) => obj[k]);
        "#;
        assert!(
            run(src).is_empty(),
            "const with explicit keyof annotation should not flag"
        );
    }

    // Regression test for issue #118 — iteration over a typed key
    // tuple (`readonly (keyof TSearch & string)[]`) inside `.map`.
    #[test]
    fn issue_118_typed_key_tuple_map() {
        let src = r#"
            type ListRouteSearch = { foo: string | null; bar: string | null };
            function filtersFromSearch<TSearch extends ListRouteSearch>(
                search: TSearch,
                filterKeys: readonly (keyof TSearch & string)[],
            ): Record<string, string | null> {
                return Object.fromEntries(
                    filterKeys.map((key) => {
                        const rawValue: unknown = search[key];
                        return [key, typeof rawValue === "string" ? rawValue : null] as const;
                    }),
                );
            }
        "#;
        let diags = run(src);
        assert!(
            diags.is_empty(),
            "expected no diagnostics, got {diags:#?}"
        );
    }
}
