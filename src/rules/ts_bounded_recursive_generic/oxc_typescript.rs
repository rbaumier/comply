//! ts-bounded-recursive-generic OXC backend — flag recursive conditional/mapped
//! types that lack a depth accumulator parameter.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSTypeAliasDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSTypeAliasDeclaration(alias) = node.kind() else {
            return;
        };

        let name = alias.id.name.as_str();
        if name.is_empty() {
            return;
        }

        // A type with no type parameters cannot be the unbounded recursive
        // conditional generic this rule targets: there is nowhere to add the
        // depth accumulator the remediation asks for. Structural self-reference
        // in a non-generic object alias (like a recursive interface) is fine.
        if alias.type_parameters.as_ref().is_none_or(|tp| tp.params.is_empty()) {
            return;
        }

        // Must be a conditional or mapped type, decided on the AST node so that
        // comment text or method-signature brackets never trigger it.
        if !is_conditional_or_mapped(&alias.type_annotation) {
            return;
        }

        // Must reference itself as a *type*. Walking the AST (not the source
        // text) ensures a property key or an indexed-access string literal that
        // merely shares the alias name does not count as a recursive call.
        if !type_annotation_references_type(&alias.type_annotation, name) {
            return;
        }

        // Must lack both a depth parameter and an accumulator parameter.
        if let Some(type_params) = &alias.type_parameters
            && (has_depth_parameter(type_params, ctx.source)
                || has_accumulator_parameter(type_params))
        {
            return;
        }

        // Exempt self-bounding recursion: a recursive call whose argument is a
        // type variable introduced by `infer` in a conditional's `extends`
        // clause. Such a variable is always a sub-part of the matched input
        // (e.g. the `Tail` of a template-literal string, or the element of
        // `Array<infer Elem>`), so it strictly shrinks each step and the
        // recursion terminates without an explicit depth counter.
        if recurses_on_infer_binding(&alias.type_annotation, name) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, alias.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Recursive type `{name}` has no depth parameter; add one to bound recursion."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Return true if `ty` is a conditional or mapped type, looking through
/// parenthesized wrappers. These are the only type forms that can blow up the
/// TypeScript type-checker through unbounded recursion.
fn is_conditional_or_mapped(ty: &oxc_ast::ast::TSType) -> bool {
    use oxc_ast::ast::TSType;
    match ty {
        TSType::TSConditionalType(_) | TSType::TSMappedType(_) => true,
        TSType::TSParenthesizedType(paren) => is_conditional_or_mapped(&paren.type_annotation),
        _ => false,
    }
}

/// Return true if `ty` contains a `TSTypeReference` whose name is `name`,
/// i.e. the alias genuinely references itself as a *type*. Property keys and
/// indexed-access string literals that share the name are not type references,
/// so they do not match. Mirrors the traversal in [`collect`].
fn type_annotation_references_type(ty: &oxc_ast::ast::TSType, name: &str) -> bool {
    use oxc_ast::ast::{TSType, TSTypeName};

    match ty {
        TSType::TSTypeReference(tref) => {
            let is_self = matches!(
                &tref.type_name,
                TSTypeName::IdentifierReference(id) if id.name.as_str() == name
            );
            is_self
                || tref.type_arguments.as_ref().is_some_and(|args| {
                    args.params.iter().any(|arg| type_annotation_references_type(arg, name))
                })
        }
        TSType::TSConditionalType(cond) => {
            type_annotation_references_type(&cond.check_type, name)
                || type_annotation_references_type(&cond.extends_type, name)
                || type_annotation_references_type(&cond.true_type, name)
                || type_annotation_references_type(&cond.false_type, name)
        }
        TSType::TSArrayType(arr) => type_annotation_references_type(&arr.element_type, name),
        TSType::TSIndexedAccessType(idx) => {
            type_annotation_references_type(&idx.object_type, name)
                || type_annotation_references_type(&idx.index_type, name)
        }
        TSType::TSUnionType(u) => {
            u.types.iter().any(|t| type_annotation_references_type(t, name))
        }
        TSType::TSIntersectionType(i) => {
            i.types.iter().any(|t| type_annotation_references_type(t, name))
        }
        TSType::TSTupleType(tuple) => tuple
            .element_types
            .iter()
            .any(|el| tuple_element_references_type(el, name)),
        TSType::TSNamedTupleMember(member) => {
            tuple_element_references_type(&member.element_type, name)
        }
        TSType::TSTypeOperatorType(op) => {
            type_annotation_references_type(&op.type_annotation, name)
        }
        TSType::TSParenthesizedType(paren) => {
            type_annotation_references_type(&paren.type_annotation, name)
        }
        TSType::TSTemplateLiteralType(tpl) => {
            tpl.types.iter().any(|t| type_annotation_references_type(t, name))
        }
        TSType::TSMappedType(mapped) => {
            type_annotation_references_type(&mapped.constraint, name)
                || mapped
                    .name_type
                    .as_ref()
                    .is_some_and(|t| type_annotation_references_type(t, name))
                || mapped
                    .type_annotation
                    .as_ref()
                    .is_some_and(|t| type_annotation_references_type(t, name))
        }
        _ => false,
    }
}

fn tuple_element_references_type(el: &oxc_ast::ast::TSTupleElement, name: &str) -> bool {
    use oxc_ast::ast::TSTupleElement;
    match el {
        TSTupleElement::TSOptionalType(opt) => {
            type_annotation_references_type(&opt.type_annotation, name)
        }
        TSTupleElement::TSRestType(rest) => {
            type_annotation_references_type(&rest.type_annotation, name)
        }
        other => other
            .as_ts_type()
            .is_some_and(|inner| type_annotation_references_type(inner, name)),
    }
}

fn has_depth_parameter(
    type_params: &oxc_ast::ast::TSTypeParameterDeclaration,
    source: &str,
) -> bool {
    for tp in &type_params.params {
        let text = &source[tp.span.start as usize..tp.span.end as usize];
        if text.contains("Depth") || text.contains("Count") {
            return true;
        }
        if text.contains("extends number") || text.contains("extends 0") {
            return true;
        }
    }
    false
}

/// Check if any type parameter defaults to an empty-collection initializer
/// (`[]`, `never`, `{}`, or `''`). Such a default marks the accumulator
/// pattern, where recursion is bounded by structural shrinkage of the input
/// each step rather than by a numeric depth counter.
fn has_accumulator_parameter(type_params: &oxc_ast::ast::TSTypeParameterDeclaration) -> bool {
    use oxc_ast::ast::{TSLiteral, TSType};
    type_params.params.iter().any(|tp| match &tp.default {
        Some(TSType::TSTupleType(tuple)) => tuple.element_types.is_empty(),
        Some(TSType::TSNeverKeyword(_)) => true,
        Some(TSType::TSTypeLiteral(literal)) => literal.members.is_empty(),
        Some(TSType::TSLiteralType(literal)) => {
            matches!(&literal.literal, TSLiteral::StringLiteral(s) if s.value.is_empty())
        }
        _ => false,
    })
}

/// Return true if `annotation` contains a recursive call to `alias_name` whose
/// type argument is a variable bound by `infer` somewhere in the annotation.
fn recurses_on_infer_binding(annotation: &oxc_ast::ast::TSType, alias_name: &str) -> bool {
    let mut infer_names = Vec::new();
    let mut self_call_args = Vec::new();
    collect(annotation, alias_name, &mut infer_names, &mut self_call_args);
    self_call_args.iter().any(|arg| infer_names.contains(arg))
}

/// Walk every nested type, recording `infer` binding names and the top-level
/// identifier type-arguments of recursive calls to `alias_name`.
fn collect<'a>(
    ty: &'a oxc_ast::ast::TSType<'a>,
    alias_name: &str,
    infer_names: &mut Vec<&'a str>,
    self_call_args: &mut Vec<&'a str>,
) {
    use oxc_ast::ast::{TSType, TSTypeName};

    match ty {
        TSType::TSInferType(infer) => {
            infer_names.push(infer.type_parameter.name.name.as_str());
        }
        TSType::TSConditionalType(cond) => {
            collect(&cond.check_type, alias_name, infer_names, self_call_args);
            collect(&cond.extends_type, alias_name, infer_names, self_call_args);
            collect(&cond.true_type, alias_name, infer_names, self_call_args);
            collect(&cond.false_type, alias_name, infer_names, self_call_args);
        }
        TSType::TSTypeReference(tref) => {
            let is_self_call = matches!(
                &tref.type_name,
                TSTypeName::IdentifierReference(id) if id.name.as_str() == alias_name
            );
            if let Some(args) = &tref.type_arguments {
                for arg in &args.params {
                    if is_self_call
                        && let TSType::TSTypeReference(arg_ref) = arg
                        && let TSTypeName::IdentifierReference(id) = &arg_ref.type_name
                        && arg_ref.type_arguments.is_none()
                    {
                        self_call_args.push(id.name.as_str());
                    }
                    collect(arg, alias_name, infer_names, self_call_args);
                }
            }
        }
        TSType::TSArrayType(arr) => {
            collect(&arr.element_type, alias_name, infer_names, self_call_args);
        }
        TSType::TSIndexedAccessType(idx) => {
            collect(&idx.object_type, alias_name, infer_names, self_call_args);
            collect(&idx.index_type, alias_name, infer_names, self_call_args);
        }
        TSType::TSUnionType(u) => {
            for t in &u.types {
                collect(t, alias_name, infer_names, self_call_args);
            }
        }
        TSType::TSIntersectionType(i) => {
            for t in &i.types {
                collect(t, alias_name, infer_names, self_call_args);
            }
        }
        TSType::TSTupleType(tuple) => {
            for el in &tuple.element_types {
                collect_tuple_element(el, alias_name, infer_names, self_call_args);
            }
        }
        TSType::TSNamedTupleMember(member) => {
            collect_tuple_element(&member.element_type, alias_name, infer_names, self_call_args);
        }
        TSType::TSTypeOperatorType(op) => {
            collect(&op.type_annotation, alias_name, infer_names, self_call_args);
        }
        TSType::TSParenthesizedType(paren) => {
            collect(&paren.type_annotation, alias_name, infer_names, self_call_args);
        }
        TSType::TSTemplateLiteralType(tpl) => {
            for t in &tpl.types {
                collect(t, alias_name, infer_names, self_call_args);
            }
        }
        TSType::TSMappedType(mapped) => {
            collect(&mapped.constraint, alias_name, infer_names, self_call_args);
            if let Some(name_type) = &mapped.name_type {
                collect(name_type, alias_name, infer_names, self_call_args);
            }
            if let Some(annotation) = &mapped.type_annotation {
                collect(annotation, alias_name, infer_names, self_call_args);
            }
        }
        _ => {}
    }
}

fn collect_tuple_element<'a>(
    el: &'a oxc_ast::ast::TSTupleElement<'a>,
    alias_name: &str,
    infer_names: &mut Vec<&'a str>,
    self_call_args: &mut Vec<&'a str>,
) {
    use oxc_ast::ast::TSTupleElement;
    match el {
        TSTupleElement::TSOptionalType(opt) => {
            collect(&opt.type_annotation, alias_name, infer_names, self_call_args);
        }
        TSTupleElement::TSRestType(rest) => {
            collect(&rest.type_annotation, alias_name, infer_names, self_call_args);
        }
        other => {
            if let Some(inner) = other.as_ts_type() {
                collect(inner, alias_name, infer_names, self_call_args);
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
    fn exempts_union_to_tuple_accumulator() {
        let src = r#"
export type UnionToTuple<
  union,
  output extends any[] = []
> = UnionToIntersection<union extends any ? (t: union) => union : never> extends (_: any) => infer elem
  ? UnionToTuple<Exclude<union, elem>, [elem, ...output]>
  : output;
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_flatten_accumulator() {
        let src = r#"
export type Flatten<
  xs extends readonly any[],
  output extends any[] = []
> = xs extends readonly [infer head, ...infer tail]
  ? Flatten<tail, [...output, ...Extract<head, readonly any[]>]>
  : output;
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_minimal_tuple_accumulator() {
        let src = "type Acc<T, Out extends any[] = []> = T extends [infer H, ...infer R] ? Acc<R, [...Out, H]> : Out;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_never_accumulator() {
        let src = "type Values<T, Out = never> = T extends [infer H, ...infer R] ? Values<R, Out | H> : Out;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_depth_parameter() {
        let src = "type Walk<T, Depth extends number = 0> = T extends object ? Walk<T[keyof T], Depth> : T;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_template_literal_string_shrink() {
        let src = r#"
export type PathParameters<
  TRoute extends string,
> = TRoute extends `${infer _Head}/{${infer _Param}}${infer Tail}`
  ? [pathParameter: string, ...pathParameters: PathParameters<Tail>]
  : [];
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_infer_from_array_element() {
        let src =
            "type Deep<T> = T extends Array<infer Elem> ? Deep<Elem> : T extends object ? string : never;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_unbounded_single_param_recursion() {
        let src = "type InfiniteLoop<T> = T extends any ? InfiniteLoop<T> : never;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_recursion_on_original_input_not_infer() {
        let src = "type Loop<T> = T extends `${infer _Head}${infer _Tail}` ? Loop<T> : never;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_non_accumulator_default() {
        let src = "type Loop<T, Fallback = string> = T extends any ? Loop<T, Fallback> : never;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn exempts_non_generic_self_referential_alias() {
        let src = r#"
export type KyInstance = {
    get: <T>(url: string) => T;
    /** Will be undefined in case ky.stop is returned, see options[0]. */
    create: (defaultOptions?: Options) => KyInstance;
    extend: (defaultOptions: Options) => KyInstance;
};
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_non_generic_recursive_object_alias() {
        let src = "type Node = { children: Node[] };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_unbounded_recursive_conditional_generic() {
        let src = "type Deep<T> = T extends object ? Deep<T[keyof T]> : T;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn exempts_property_key_name_collision() {
        let src = "export type value<T> = T extends { value: infer V } ? V : never;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_indexed_access_string_literal_name_collision() {
        let src =
            r#"export type input<T> = T extends { _zod: { input: any } } ? T["_zod"]["input"] : unknown;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_self_reference_with_string_literal_index_in_body() {
        let src = "type Deep<T> = T extends { next: infer N } ? Deep<T[\"next\"]> : T;";
        assert_eq!(run_on(src).len(), 1);
    }
}
