use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, TSType, TSTypeName, TSTypeOperatorOperator};
use std::sync::Arc;

/// Generic type names whose instances are array-like or iterable, so iterating
/// them with `for...in` (which enumerates own enumerable string keys) is the
/// same bug as on a plain array — `for...of` is the intended form.
const ITERABLE_TYPE_NAMES: &[&str] = &[
    "Array",
    "ReadonlyArray",
    "Set",
    "ReadonlySet",
    "Map",
    "ReadonlyMap",
    "Int8Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "Int16Array",
    "Uint16Array",
    "Int32Array",
    "Uint32Array",
    "Float32Array",
    "Float64Array",
    "BigInt64Array",
    "BigUint64Array",
];

/// Member-call names whose result is always a fresh array.
const ARRAY_PRODUCING_METHODS: &[&str] = &[
    "map", "filter", "slice", "concat", "flat", "flatMap", "splice", "fill", "reverse", "sort",
    "split", "from", "of",
];

/// True when `ty` is an array type (`T[]`, `readonly T[]`) or a reference to a
/// built-in iterable container (`Array<T>`, `Set<T>`, `Map<K, V>`, a typed
/// array, …). A plain object / `Record` / index-signature type is not iterable
/// and returns `false`.
fn type_is_iterable(ty: &TSType) -> bool {
    match ty {
        TSType::TSArrayType(_) => true,
        // `readonly T[]` wraps the array type; `keyof T[]` / `unique` name the
        // keys, not the collection, so only unwrap the `readonly` operator.
        TSType::TSTypeOperatorType(op) if op.operator == TSTypeOperatorOperator::Readonly => {
            type_is_iterable(&op.type_annotation)
        }
        TSType::TSTypeReference(type_ref) => {
            let TSTypeName::IdentifierReference(id) = &type_ref.type_name else {
                return false;
            };
            ITERABLE_TYPE_NAMES.contains(&id.name.as_str())
        }
        _ => false,
    }
}

/// True when `expr` evaluates to a freshly produced array/iterable: an array
/// literal, `new Array(...)`, `Array.from(...)` / `Array.of(...)`, or a chained
/// array method (`xs.map(...)`, `xs.filter(...)`, …).
fn expr_produces_array(expr: &Expression) -> bool {
    match expr.without_parentheses() {
        Expression::ArrayExpression(_) => true,
        Expression::NewExpression(new) => {
            matches!(&new.callee, Expression::Identifier(id) if id.name == "Array")
        }
        Expression::CallExpression(call) => match &call.callee {
            Expression::StaticMemberExpression(member) => {
                ARRAY_PRODUCING_METHODS.contains(&member.property.name.as_str())
            }
            _ => false,
        },
        _ => false,
    }
}

/// True when the `for...in` operand is genuinely an array/iterable: an array
/// literal, or a bare identifier whose binding is annotated with an iterable
/// type (`T[]`, `Array<T>`, `Set`, `Map`, …) or initialized from an
/// array-producing expression. A plain object / `Record` / index-signature
/// binding (the idiomatic `for...in` target) is not, so it is left unflagged.
fn operand_is_iterable<'a>(
    operand: &Expression<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::BindingPattern;

    if expr_produces_array(operand) {
        return true;
    }

    let Expression::Identifier(ident) = operand.without_parentheses() else {
        return false;
    };
    let Some(ref_id) = ident.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();

    // Only trust a declaration whose binding is the bare identifier itself; a
    // destructured binding's real type is an element/property of the annotation,
    // not the annotation.
    fn pattern_is_bare_identifier(pattern: &BindingPattern) -> bool {
        matches!(pattern, BindingPattern::BindingIdentifier(_))
    }

    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        match kind {
            AstKind::VariableDeclarator(decl) if pattern_is_bare_identifier(&decl.id) => {
                if let Some(ann) = &decl.type_annotation {
                    return type_is_iterable(&ann.type_annotation);
                }
                return decl.init.as_ref().is_some_and(expr_produces_array);
            }
            AstKind::FormalParameter(param) if pattern_is_bare_identifier(&param.pattern) => {
                return param
                    .type_annotation
                    .as_ref()
                    .is_some_and(|ann| type_is_iterable(&ann.type_annotation));
            }
            _ => {}
        }
    }
    false
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ForInStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ForInStatement(stmt) = node.kind() else {
            return;
        };
        if !operand_is_iterable(&stmt.right, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, stmt.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`for...in` on an array/iterable — use `for...of` instead.".into(),
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_for_in_over_array_literal() {
        assert_eq!(run_on("for (const x in [1, 2, 3]) {}").len(), 1);
    }

    #[test]
    fn flags_for_in_over_typed_array_binding() {
        let src = "const arr: number[] = [1, 2]; for (const i in arr) {}";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    #[test]
    fn flags_for_in_over_readonly_array_binding() {
        let src = "const arr: readonly number[] = [1, 2]; for (const i in arr) {}";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    #[test]
    fn flags_for_in_over_array_generic_binding() {
        let src = "const xs: Array<string> = []; for (const i in xs) {}";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    #[test]
    fn flags_for_in_over_set_binding() {
        let src = "const s: Set<string> = new Set(); for (const k in s) {}";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    #[test]
    fn flags_for_in_over_array_initialized_binding() {
        let src = "const items = list.map(x => x); for (const i in items) {}";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    #[test]
    fn flags_for_in_over_array_typed_param() {
        let src = "function f(values: number[]) { for (const i in values) {} }";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    // Regression for #4881: konva `eventListeners` is an index-signature object
    // (`{ [index: string]: Array<...> }`) — a plain dict, not an array. `for...in`
    // over its keys is the correct idiom. The `list` substring in the name must
    // not classify it as an array.
    #[test]
    fn allows_for_in_over_index_signature_record() {
        let src = "for (const t in this.eventListeners) {}";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // Regression for #4881: melonJS `_valuesEnd` / `_valuesStart` are
    // `Record<string, unknown>` — plain maps, not arrays.
    #[test]
    fn allows_for_in_over_record_member() {
        let src = "for (const property in this._valuesEnd) {}";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_for_in_over_record_typed_binding() {
        let src = "const valuesEnd: Record<string, unknown> = {}; for (const p in valuesEnd) {}";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_for_in_over_object_initialized_binding() {
        let src = "const itemsList = {}; for (const k in itemsList) {}";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_for_in_over_plain_object() {
        assert!(run_on("for (const key in obj) {}").is_empty());
    }

    // Negative space: `keyof number[]` is the array's key union (`number`), not
    // an array — only the `readonly` type operator unwraps to the inner array.
    #[test]
    fn allows_for_in_over_keyof_array_typed_binding() {
        let src = "const k: keyof number[] = 0; for (const i in k) {}";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // Negative space: a bare name with an array-hinting word but no type/init
    // evidence is not flagged — names do not determine type.
    #[test]
    fn allows_for_in_over_unresolved_array_named_binding() {
        assert!(run_on("for (const x in myArray) {}").is_empty());
    }

    #[test]
    fn allows_for_of() {
        assert!(run_on("for (const x of myArray) {}").is_empty());
    }
}
