//! ts-no-unnecessary-type-constraint oxc backend — flag `<T extends any>` or
//! `<T extends unknown>`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{TSType, TSTypeName};
use oxc_span::{GetSpan, Span};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSTypeParameter]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSTypeParameter(param) = node.kind() else { return };

        // A `TSTypeParameterDeclaration` holds a generic parameter. A
        // `TSInferType` holds an `infer T extends C` binding. The rule targets
        // only the first: an `infer` constraint bounds what the enclosing
        // conditional type infers. It does not restrict a caller-supplied type
        // argument.
        let declaration = semantic.nodes().parent_node(node.id());
        if !matches!(declaration.kind(), AstKind::TSTypeParameterDeclaration(_)) {
            return;
        }

        let Some(constraint) = &param.constraint else { return };
        let keyword = match constraint {
            TSType::TSAnyKeyword(_) => "any",
            TSType::TSUnknownKeyword(_) => "unknown",
            _ => return,
        };

        // The bound is intentional, not redundant, when the parameter drives a
        // conditional type within its declaring scope: `T extends R ? A : B`
        // (plus the `[T] extends [U]` distribution-control variant). In those
        // type-level utilities `extends unknown`/`extends any` pins the
        // parameter against a naked-type-parameter conditional, so it is not the
        // no-op the rule otherwise targets.
        let name = param.name.name.as_str();
        if used_in_conditional_type(declaration, name, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, constraint.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Unnecessary `extends {keyword}` constraint — \
                 all types already extend `{keyword}`."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// Return true when `name` appears as the check or extends type of a conditional
/// type bound to *this* type parameter. `declaration` is the parameter's
/// `TSTypeParameterDeclaration`; its parent is the owner (function, type alias,
/// interface, class, method, …) whose span bounds where the parameter name is
/// visible. A conditional is considered only when it lives in that span and is
/// not nested under an inner `TSTypeParameterDeclaration` that re-declares
/// `name` — that inner binding shadows the outer one, so its conditional says
/// nothing about our parameter.
fn used_in_conditional_type(
    declaration: &oxc_semantic::AstNode,
    name: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let scope = semantic.nodes().parent_node(declaration.id()).kind().span();
    semantic.nodes().iter().any(|n| {
        let AstKind::TSConditionalType(cond) = n.kind() else { return false };
        let span = cond.span;
        span.start >= scope.start
            && span.end <= scope.end
            && (type_references(&cond.check_type, name)
                || type_references(&cond.extends_type, name))
            && !is_shadowed(span, scope, name, semantic)
    })
}

/// Return true when an inner generic re-binds `name` and its scope encloses the
/// conditional at `cond_span` — that inner binding shadows the outer parameter,
/// so the conditional says nothing about it. The shadowing scope is the owner of
/// a `TSTypeParameterDeclaration` (e.g. the inner function type), whose span
/// contains the conditional yet is strictly inside the outer declaration.
fn is_shadowed(
    cond_span: Span,
    outer: Span,
    name: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    semantic.nodes().iter().any(|n| {
        let AstKind::TSTypeParameterDeclaration(decl) = n.kind() else { return false };
        // Only inner generics inside the outer owner can shadow the parameter.
        if decl.span.start < outer.start || decl.span.end > outer.end {
            return false;
        }
        if !decl.params.iter().any(|p| p.name.name.as_str() == name) {
            return false;
        }
        let inner = semantic.nodes().parent_node(n.id()).kind().span();
        // The inner generic is nested below the outer owner and its scope
        // encloses the conditional.
        inner.start > outer.start
            && inner.start <= cond_span.start
            && inner.end >= cond_span.end
    })
}

/// Return true when `ty` contains a `TSTypeReference` whose name is `name`.
/// Walks the common composite type forms so a parameter wrapped in a tuple,
/// array, union, etc. (e.g. `[T] extends [unknown]`) is still recognised.
fn type_references(ty: &TSType, name: &str) -> bool {
    match ty {
        TSType::TSTypeReference(tref) => {
            let is_self = matches!(
                &tref.type_name,
                TSTypeName::IdentifierReference(id) if id.name.as_str() == name
            );
            is_self
                || tref.type_arguments.as_ref().is_some_and(|args| {
                    args.params.iter().any(|arg| type_references(arg, name))
                })
        }
        TSType::TSArrayType(arr) => type_references(&arr.element_type, name),
        TSType::TSIndexedAccessType(idx) => {
            type_references(&idx.object_type, name) || type_references(&idx.index_type, name)
        }
        TSType::TSParenthesizedType(paren) => type_references(&paren.type_annotation, name),
        TSType::TSUnionType(u) => u.types.iter().any(|t| type_references(t, name)),
        TSType::TSIntersectionType(i) => i.types.iter().any(|t| type_references(t, name)),
        TSType::TSTupleType(tuple) => tuple
            .element_types
            .iter()
            .any(|el| el.as_ts_type().is_some_and(|inner| type_references(inner, name))),
        TSType::TSNamedTupleMember(member) => member
            .element_type
            .as_ts_type()
            .is_some_and(|inner| type_references(inner, name)),
        TSType::TSTypeOperatorType(op) => type_references(&op.type_annotation, name),
        _ => false,
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
    fn flags_function_extends_unknown() {
        let d = run_on("function f<T extends unknown>() {}");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "ts-no-unnecessary-type-constraint");
    }

    #[test]
    fn flags_interface_extends_any() {
        let d = run_on("interface I<T extends any> {}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_type_alias_extends_unknown() {
        let d = run_on("type Bar<T extends unknown> = {};");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_class_method_extends_any() {
        let d = run_on("class Baz<T extends any> { qux<U extends any>() {} }");
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn allows_no_constraint() {
        assert!(run_on("function f<T>() {}").is_empty());
    }

    #[test]
    fn allows_real_constraint() {
        assert!(run_on("function f<T extends string>() {}").is_empty());
    }

    // #5278: `extends unknown`/`extends any` is intentional in conditional-type
    // testing utilities (typebox `test/common/assert.ts`).
    #[test]
    fn allows_conditional_naked_param() {
        // `Left extends Right ? ... : ...` — both params drive the conditional.
        let src =
            "type TExtendsExpect<Left extends unknown, Right extends unknown> = Left extends Right ? true : false;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_tuple_wrapped_conditional() {
        // `[T] extends [unknown] ? 1 : 0` distribution-control idiom.
        let src = "type Eq<T extends unknown> = [T] extends [unknown] ? 1 : 0;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_conditional_in_function_signature() {
        let src = "export function IsExtends<Left extends unknown, Right extends unknown>(_expect: Left extends Right ? true : false) {}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_tuple_conditional_in_function_signature() {
        // typebox IsExtendsWhenLeftIsNever shape: `[X<L,R>] extends [true] ? ...`.
        let src = "export function IsExtendsWhenLeftIsNever<Left extends unknown, Right extends unknown>(_expect: [Left] extends [Right] ? true : false) {}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_param_not_in_conditional() {
        // A sibling param used in a conditional must not exempt an unrelated one:
        // only `B` participates, so `A`'s `extends unknown` is a genuine no-op.
        let src = "type T<A extends unknown, B extends unknown> = B extends string ? 1 : 0;";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_outer_param_shadowed_by_inner_conditional() {
        // The inner `T` is a distinct binding; its conditional must not exempt
        // the outer `T extends unknown`, which is a genuine no-op.
        let src = "type Outer<T extends unknown> = { inner: <T>(x: T extends string ? 1 : 0) => void };";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_indexed_access_check_type() {
        // `T['k'] extends ... ? ...` reaches the param through indexed access.
        let src = "type X<T extends unknown> = T['k'] extends string ? 1 : 0;";
        assert!(run_on(src).is_empty());
    }

    // #6871: `infer T extends C` is a bounded-inference binding, not a generic
    // parameter (typebox `src/schema/static/schema.ts`).
    #[test]
    fn allows_infer_binding_extends_unknown() {
        let src =
            "type ExtractConst<T> = T extends { value: infer Value extends unknown } ? Value : never;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_infer_binding_in_tuple_pattern() {
        let src = "type ExtractItem<T> = T extends [infer First extends unknown, ...infer Rest extends unknown[]] ? First : never;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_infer_binding_extends_any() {
        let src = "type Unwrap<T> = T extends Promise<infer U extends any> ? U : T;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_generic_param_beside_infer_binding() {
        // The `infer` bindings are out of scope, but `U extends unknown` drives
        // no conditional, so it stays a genuine no-op. The column pins which of
        // the three `unknown` bounds is reported: the first one, `U`'s. That
        // also proves the tuple `infer` bounds parsed and were then skipped,
        // rather than never reaching the rule at all.
        let src = "type Head<U extends unknown, T> = [U, T extends [infer First extends unknown, ...infer Rest extends unknown[]] ? First : never];";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].line, 1);
        assert_eq!(d[0].column, src.find("unknown").unwrap() + 1);
    }
}
