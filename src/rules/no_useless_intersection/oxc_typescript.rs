use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::TSType;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSIntersectionType]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSIntersectionType(intersection) = node.kind() else {
            return;
        };
        let has_unknown = intersection
            .types
            .iter()
            .any(|ty| matches!(ty, TSType::TSUnknownKeyword(_)));
        let has_never = intersection
            .types
            .iter()
            .any(|ty| matches!(ty, TSType::TSNeverKeyword(_)));
        if !has_unknown && !has_never {
            return;
        }
        // `unknown &` as the leading operand is a deliberate TypeScript trick to
        // defer/distribute conditional-type evaluation over generic parameters,
        // not a no-op. Exempt it when used in those generic-aware contexts.
        let leads_with_unknown =
            matches!(intersection.types.first(), Some(TSType::TSUnknownKeyword(_)));
        if leads_with_unknown && is_deferral_trick(node, semantic) {
            return;
        }
        // TypeScript drops `unknown` from an intersection, but the operand still
        // steers how the checker resolves the siblings. Report the `unknown`
        // member only when every sibling is a plain written-out type, which the
        // extra operand cannot change. A `never` member collapses the whole
        // intersection to `never`, so it still flags.
        let unknown_is_load_bearing = has_unknown
            && !has_never
            && intersection.types.iter().any(unknown_steers_operand);
        if unknown_is_load_bearing {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, intersection.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Intersection with `unknown` or `never` is useless — simplify it.".into(),
            severity: super::META.severity,
            span: None,
        });
    }
}

/// True when an `unknown` sibling changes how TypeScript resolves the operand
/// `ty`, instead of being dropped as the intersection identity:
///
/// - object shapes (`{ a: number }`, mapped types): `& unknown` makes the
///   checker resolve and display the flattened members eagerly, the
///   `Prettify`/`Compute` idiom;
/// - types read out of value space (`typeof X`, `InstanceType<typeof X>`): the
///   shape is inferred from a value declaration instead of written out, and
///   `& unknown` bounds how deep the checker instantiates that inference at
///   the declaration site.
fn unknown_steers_operand(ty: &TSType) -> bool {
    matches!(ty, TSType::TSMappedType(_) | TSType::TSTypeLiteral(_)) || reads_value_space(ty)
}

/// True when `ty` is a `typeof` query, or carries one in a position that makes
/// the whole type inherit the query's shape, through any parentheses:
///
/// - a type argument (`InstanceType<typeof Component>`);
/// - the object side of an element lookup (`(typeof list)[number]`) — the index
///   side is excluded because `Config[typeof key]` takes its shape from
///   `Config`, which the author writes out.
///
/// The set is closed: every other wrapper (`keyof`, `readonly`, arrays, tuples,
/// unions) keeps its `& unknown` reported.
fn reads_value_space(ty: &TSType) -> bool {
    match ty {
        TSType::TSTypeQuery(_) => true,
        TSType::TSParenthesizedType(parenthesized) => {
            reads_value_space(&parenthesized.type_annotation)
        }
        TSType::TSTypeReference(reference) => reference
            .type_arguments
            .as_ref()
            .is_some_and(|arguments| arguments.params.iter().any(reads_value_space)),
        TSType::TSIndexedAccessType(indexed) => reads_value_space(&indexed.object_type),
        _ => false,
    }
}

/// True when a `unknown &`-leading intersection sits in a context where the
/// `unknown &` prefix is the documented TypeScript trick to defer or distribute
/// type evaluation over generic parameters, rather than a no-op intersection:
///
/// - the check type of a conditional type (`unknown & T extends … ? … : …`), or
/// - the body of a generic type alias (`type A<P> = unknown & Foo<P>`).
fn is_deferral_trick<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let intersection_span = node.kind().span();
    let parent = semantic.nodes().parent_node(node.id());
    match parent.kind() {
        AstKind::TSConditionalType(conditional) => {
            conditional.check_type.span() == intersection_span
        }
        AstKind::TSTypeAliasDeclaration(alias) => {
            alias.type_parameters.is_some()
                && alias.type_annotation.span() == intersection_span
        }
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
    fn flags_intersection_with_unknown() {
        assert_eq!(run_on("type X = Foo & unknown;").len(), 1);
    }

    #[test]
    fn flags_unknown_on_left() {
        assert_eq!(run_on("type X = unknown & Foo;").len(), 1);
    }

    #[test]
    fn flags_intersection_with_never() {
        assert_eq!(run_on("type X = Foo & never;").len(), 1);
    }

    #[test]
    fn allows_intersection_with_any() {
        assert!(run_on("type X = Foo & any;").is_empty());
    }

    #[test]
    fn allows_any_on_left() {
        assert!(run_on("type X = any & Foo;").is_empty());
    }

    #[test]
    fn allows_normal_intersection() {
        assert!(run_on("type X = Foo & Bar;").is_empty());
    }

    #[test]
    fn no_false_positive_on_any_prefix() {
        assert!(run_on("type X = anything & Foo;").is_empty());
    }

    #[test]
    fn allows_unknown_prefix_on_conditional_check_type() {
        let src = "export type UseSpringProps<Props extends object = any> = unknown &\n  PickAnimated<Props> extends infer State\n  ? State extends Lookup\n    ? Remap<ControllerUpdate<State> & { ref?: SpringRef<State> }>\n    : never\n  : never;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_unknown_prefix_in_generic_type_alias() {
        let src = "export type ControllerUpdate<\n  State extends Lookup = Lookup,\n  Item = undefined,\n> = unknown & ToProps<State> & ControllerProps<State, Item>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_non_leading_unknown_in_generic_type_alias() {
        assert_eq!(run_on("type X<T> = Foo<T> & unknown;").len(), 1);
    }

    #[test]
    fn allows_mapped_type_and_unknown_prettify_idiom() {
        let src = "export type Compute<T> = T extends Function ? T : { [K in keyof T]: T[K] } & unknown;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_deep_mapped_type_and_unknown_prettify_idiom() {
        let src = "export type ComputeDeep<T> = T extends Function ? T : { [K in keyof T]: ComputeDeep<T[K]> } & unknown;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_object_literal_and_unknown_prettify_idiom() {
        assert!(run_on("type X = { a: number } & unknown;").is_empty());
    }

    #[test]
    fn flags_type_reference_and_unknown() {
        assert_eq!(run_on("type Y = Bar & unknown;").len(), 1);
    }

    #[test]
    fn allows_instance_type_of_component_query_and_unknown() {
        let src = "import type TabBar from './tab-bar.vue';\nexport type TabBarInstance = InstanceType<typeof TabBar> & unknown;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_bare_type_query_and_unknown() {
        assert!(run_on("type X = typeof tabBarProps & unknown;").is_empty());
    }

    #[test]
    fn flags_generic_instantiation_without_type_query_and_unknown() {
        assert_eq!(run_on("type X = InstanceType<TabBar> & unknown;").len(), 1);
    }

    #[test]
    fn allows_nested_generic_type_query_and_unknown() {
        let src = "type X = Ref<InstanceType<typeof TabBar>> & unknown;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_indexed_access_on_type_query_and_unknown() {
        let src = "type X = InstanceType<(typeof mod)['default']> & unknown;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_indexed_access_without_type_query_and_unknown() {
        assert_eq!(
            run_on("type X = InstanceType<Mod['default']> & unknown;").len(),
            1
        );
    }

    #[test]
    fn flags_indexed_access_with_type_query_on_index_side_and_unknown() {
        assert_eq!(run_on("type X = Config[typeof key] & unknown;").len(), 1);
    }

    #[test]
    fn flags_array_of_type_query_and_unknown() {
        assert_eq!(run_on("type X = (typeof handler)[] & unknown;").len(), 1);
    }

    #[test]
    fn flags_keyof_type_query_and_unknown() {
        assert_eq!(run_on("type X = keyof typeof config & unknown;").len(), 1);
    }

    #[test]
    fn flags_type_query_and_never() {
        assert_eq!(run_on("type X = typeof tabBarProps & never;").len(), 1);
    }

    #[test]
    fn flags_object_literal_and_never() {
        assert_eq!(run_on("type Z = { a: number } & never;").len(), 1);
    }

    #[test]
    fn flags_object_literal_with_both_unknown_and_never() {
        assert_eq!(run_on("type Z = { a: number } & unknown & never;").len(), 1);
    }
}
