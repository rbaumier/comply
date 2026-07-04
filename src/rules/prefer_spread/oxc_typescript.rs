//! prefer-spread — OXC backend.
//! Flags `Array.from()`, `[].concat()`, and `.slice()` / `.slice(0)`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, ClassElement, Expression, PropertyKey, TSType, TSTypeName};
use std::sync::Arc;

/// ECMAScript TypedArray constructor names. `TypedArray.prototype.slice()`
/// returns a new TypedArray of the same concrete type, whereas `[...x]` yields a
/// plain `number[]` — a real type change, so spread is not an equivalent rewrite
/// and must not be suggested for a TypedArray receiver.
const TYPED_ARRAY_TYPE_NAMES: &[&str] = &[
    "Uint8Array",
    "Uint8ClampedArray",
    "Uint16Array",
    "Uint32Array",
    "Int8Array",
    "Int16Array",
    "Int32Array",
    "Float32Array",
    "Float64Array",
    "BigInt64Array",
    "BigUint64Array",
];

fn type_is_typed_array(ty: &TSType) -> bool {
    let TSType::TSTypeReference(type_ref) = ty else {
        return false;
    };
    let TSTypeName::IdentifierReference(id) = &type_ref.type_name else {
        return false;
    };
    TYPED_ARRAY_TYPE_NAMES.contains(&id.name.as_str())
}

/// True when `expr` evaluates to a freshly produced TypedArray: `new Uint8Array(…)`
/// or a `Uint8Array.from(…)` / `Uint8Array.of(…)` factory. A plain array is never
/// produced this way, so inferring the type from such an initializer cannot
/// over-skip a genuine plain-array receiver.
fn expr_is_typed_array(expr: &Expression) -> bool {
    match expr.without_parentheses() {
        Expression::NewExpression(new) => matches!(
            &new.callee,
            Expression::Identifier(id) if TYPED_ARRAY_TYPE_NAMES.contains(&id.name.as_str())
        ),
        Expression::CallExpression(call) => {
            matches!(&call.callee, Expression::StaticMemberExpression(m)
                if matches!(&m.object, Expression::Identifier(id)
                    if TYPED_ARRAY_TYPE_NAMES.contains(&id.name.as_str()))
                    && matches!(m.property.name.as_str(), "from" | "of"))
        }
        _ => false,
    }
}

fn pattern_is_bare_identifier(pattern: &BindingPattern) -> bool {
    matches!(pattern, BindingPattern::BindingIdentifier(_))
}

fn key_name<'a>(key: &'a PropertyKey) -> Option<&'a str> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
        PropertyKey::StringLiteral(s) => Some(s.value.as_str()),
        _ => None,
    }
}

/// True when the `.slice()` receiver resolves in-file to a binding or class
/// field whose TS type annotation is a TypedArray. Covers local `const`/`let`
/// declarations, formal parameters (incl. constructor parameter properties), and
/// `this.field` class members. Cross-file imported symbols are not resolvable
/// here and are intentionally left flagged.
fn receiver_is_typed_array<'a>(
    object: &Expression<'a>,
    call_node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    match object {
        Expression::Identifier(ident) => {
            let Some(ref_id) = ident.reference_id.get() else {
                return false;
            };
            let scoping = semantic.scoping();
            let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
                return false;
            };
            let decl_node = scoping.symbol_declaration(sym_id);
            let nodes = semantic.nodes();
            for kind in
                std::iter::once(nodes.kind(decl_node)).chain(nodes.ancestor_kinds(decl_node))
            {
                match kind {
                    AstKind::VariableDeclarator(decl)
                        if pattern_is_bare_identifier(&decl.id) =>
                    {
                        if let Some(ann) = &decl.type_annotation {
                            return type_is_typed_array(&ann.type_annotation);
                        }
                        return decl.init.as_ref().is_some_and(expr_is_typed_array);
                    }
                    AstKind::FormalParameter(param)
                        if pattern_is_bare_identifier(&param.pattern) =>
                    {
                        return param
                            .type_annotation
                            .as_ref()
                            .is_some_and(|ann| type_is_typed_array(&ann.type_annotation));
                    }
                    _ => {}
                }
            }
            false
        }
        Expression::StaticMemberExpression(inner)
            if matches!(inner.object, Expression::ThisExpression(_)) =>
        {
            let field = inner.property.name.as_str();
            let nodes = semantic.nodes();
            for kind in nodes.ancestor_kinds(call_node.id()) {
                if let AstKind::Class(class) = kind {
                    return class.body.body.iter().any(|el| {
                        matches!(el, ClassElement::PropertyDefinition(p)
                            if key_name(&p.key) == Some(field)
                                && p.type_annotation
                                    .as_ref()
                                    .is_some_and(|ann| type_is_typed_array(&ann.type_annotation)))
                    });
                }
            }
            false
        }
        _ => false,
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

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let prop = member.property.name.as_str();

        // Array.from(...) — single iterable arg, not an object literal
        if prop == "from" {
            let Expression::Identifier(obj) = &member.object else { return };
            if obj.name.as_str() != "Array" {
                return;
            }
            if call.arguments.len() >= 2 {
                return;
            }
            if let Some(first) = call.arguments.first()
                && let Some(Expression::ObjectExpression(_)) = first.as_expression() {
                    return;
                }
            let (line, column) =
                byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Prefer the spread operator over `Array.from(...)`.".into(),
                severity: Severity::Warning,
                span: None,
            });
            return;
        }

        // [].concat(...) — only when the receiver AND every argument are array
        // literals. `Array#concat` follows IsConcatSpreadable: an array argument
        // is flattened in, a non-array argument is appended as a single element,
        // so `[].concat(existing, value)` normalizes a mix of arrays and scalars.
        // Spread is a behavior-preserving rewrite only when every operand is a
        // known array (`[...3]` throws, `[...(/re/)]` throws, `[...'ab']` splits
        // the string); a scalar/unknown argument has no spread equivalent.
        if prop == "concat" {
            let all_args_are_array_literals = call.arguments.iter().all(|arg| {
                matches!(arg.as_expression(), Some(Expression::ArrayExpression(_)))
            });
            if matches!(&member.object, Expression::ArrayExpression(_))
                && all_args_are_array_literals
            {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Prefer the spread operator over `Array#concat(...)`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            return;
        }

        // .slice() or .slice(0) — shallow copy pattern
        if prop == "slice" {
            let is_copy = call.arguments.is_empty()
                || (call.arguments.len() == 1
                    && call.arguments.first().is_some_and(|arg| {
                        matches!(arg.as_expression(), Some(Expression::NumericLiteral(n)) if n.value == 0.0)
                    }));
            if is_copy {
                // TypedArray `.slice()` returns a same-type TypedArray; the
                // spread rewrite would change the type to plain `number[]`.
                if receiver_is_typed_array(&member.object, node, semantic) {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Prefer the spread operator over `Array#slice()`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_array_from() {
        let d = run_on("const arr = Array.from(iterable);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Array.from"));
    }

    #[test]
    fn flags_concat_array_literal() {
        let d = run_on("const combined = [1,2].concat([3,4]);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("concat"));
    }

    #[test]
    fn allows_concat_identifier() {
        assert!(run_on("const combined = arr.concat(other);").is_empty());
    }

    #[test]
    fn allows_concat_array_literal_with_identifier_args() {
        // `[].concat(existing, value)` is the polymorphic array-normalization
        // idiom; the args may be scalars, so spread is not equivalent.
        assert!(run_on("const m = [].concat(existing, value);").is_empty());
    }

    #[test]
    fn allows_concat_array_literal_with_mixed_scalar_arg() {
        // A non-array-literal argument (`3`) is appended as a single element by
        // concat; `[...3]` would throw, so spread is not equivalent.
        assert!(run_on("const m = [].concat([1, 2], 3);").is_empty());
    }

    #[test]
    fn allows_concat_array_literal_receiver_with_identifier_arg() {
        assert!(run_on("const m = [1, 2].concat(other);").is_empty());
    }

    #[test]
    fn allows_array_from_with_map_fn() {
        assert!(run_on("Array.from({ length: 3 }, (_, i) => i);").is_empty());
    }

    #[test]
    fn allows_array_from_object_literal() {
        assert!(run_on("Array.from({ length: 3 });").is_empty());
    }

    #[test]
    fn flags_slice_empty() {
        let d = run_on("const copy = arr.slice();");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("slice"));
    }

    #[test]
    fn flags_slice_zero() {
        let d = run_on("const copy = arr.slice(0);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_slice_with_args() {
        assert!(run_on("const sub = arr.slice(1, 3);").is_empty());
    }

    #[test]
    fn allows_spread() {
        assert!(run_on("const arr = [...iterable];").is_empty());
    }

    #[test]
    fn allows_slice_on_typed_array_param() {
        // `constants: Uint32Array` — `.slice()` returns a Uint32Array; spread
        // would change the type to `number[]`.
        let src = "function f(constants: Uint32Array) { return constants.slice(); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_slice_on_typed_array_local_const() {
        let src = "const buf: Uint8Array = read(); const copy = buf.slice();";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_slice_on_typed_array_new_initializer() {
        // No annotation, but `new Uint8Array(...)` initializer infers the type.
        let src = "const buf = new Uint8Array(8); const copy = buf.slice();";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_slice_on_typed_array_from_factory() {
        let src = "const buf = Uint32Array.from(xs); const copy = buf.slice();";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_slice_on_typed_array_class_field() {
        let src = "class C { state: Uint8Array = new Uint8Array(); copy() { return this.state.slice(); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_slice_on_number_array_annotation() {
        // Plain `number[]` receiver — spread is a valid replacement, still flagged.
        let src = "const a: number[] = [1, 2, 3]; const b = a.slice();";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("slice"));
    }
}
