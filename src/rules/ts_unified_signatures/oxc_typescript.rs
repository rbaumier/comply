//! ts-unified-signatures OXC backend — flag adjacent function overload signatures
//! in interfaces/type literals that share the same name.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    FormalParameters, PropertyKey, TSCallSignatureDeclaration, TSLiteral, TSSignature,
    TSType, TSTypeAnnotation, TSTypeParameterDeclaration,
};
use oxc_span::GetSpan;
use rustc_hash::FxHashSet;
use rustc_hash::FxHashMap;
use std::sync::Arc;

pub struct Check;

/// The type parameters, parameter list, and declared return type of an overload
/// signature — the facets that determine whether a group of overloads can be
/// merged. Lets the unifiability heuristics treat call signatures and named method
/// signatures uniformly.
struct SigShape<'a> {
    type_params: Option<&'a TSTypeParameterDeclaration<'a>>,
    params: &'a FormalParameters<'a>,
    return_type: Option<&'a TSTypeAnnotation<'a>>,
}

/// The single string-literal value typing a call signature's first parameter,
/// e.g. `"/geocode"` for `(path: "/geocode"): T`. `None` when the signature does
/// not have exactly one parameter, or that parameter is not a string-literal type.
fn first_param_string_literal<'a>(call: &'a TSCallSignatureDeclaration<'a>) -> Option<&'a str> {
    let [param] = call.params.items.as_slice() else {
        return None;
    };
    let TSType::TSLiteralType(lit) = &param.type_annotation.as_ref()?.type_annotation else {
        return None;
    };
    match &lit.literal {
        TSLiteral::StringLiteral(s) => Some(s.value.as_str()),
        _ => None,
    }
}

/// Path-discriminated dispatchers (e.g. the Azure SDK `Routes` interface) map a
/// distinct string-literal path to a distinct return type per overload. Unifying
/// them would erase the per-path return-type inference, so they are not a smell.
/// True when every call signature is typed by a *distinct* string literal.
fn call_signatures_are_path_discriminated<'a>(
    calls: &[&'a TSCallSignatureDeclaration<'a>],
) -> bool {
    let mut literals = FxHashSet::default();
    for call in calls {
        let Some(literal) = first_param_string_literal(call) else {
            return false;
        };
        if !literals.insert(literal) {
            return false;
        }
    }
    true
}

/// The source text of a signature's declared return type, or `None` when it has
/// no annotation (an inferred return is treated as a distinct return type).
fn return_type_text<'a>(shape: &SigShape<'a>, source: &'a str) -> Option<&'a str> {
    let annotation = shape.return_type?;
    Some(&source[annotation.type_annotation.span().start as usize..annotation.type_annotation.span().end as usize])
}

/// The source text of a signature's parameter list (including the parentheses),
/// e.g. `(pinia: Pinia | undefined)`.
fn params_text<'a>(shape: &SigShape<'a>, source: &'a str) -> &'a str {
    &source[shape.params.span.start as usize..shape.params.span.end as usize]
}

/// The source text of a signature's type-parameter list (including the angle
/// brackets), e.g. `<$Output extends AsyncIterable<any, void, any>>`, or `None`
/// when the signature declares none. Two overloads whose parameter lists are
/// textually identical can still be distinguished by their type-parameter
/// constraints, so this is part of a signature's input identity.
fn type_params_text<'a>(shape: &SigShape<'a>, source: &'a str) -> Option<&'a str> {
    let decl = shape.type_params?;
    Some(&source[decl.span.start as usize..decl.span.end as usize])
}

/// The source text of the type annotation of the parameter at `index`, or `None`
/// when the signature has no parameter there or that parameter is untyped. Names
/// are excluded so positions compare by type alone.
fn param_type_text<'a>(shape: &SigShape<'a>, index: usize, source: &'a str) -> Option<&'a str> {
    let span = shape
        .params
        .items
        .get(index)?
        .type_annotation
        .as_ref()?
        .type_annotation
        .span();
    Some(&source[span.start as usize..span.end as usize])
}

/// How many of the first `count` parameter positions have a parameter type text not
/// shared by every signature in the group. A unified signature can union at most one
/// position, so two or more differing positions (the equal-arity case) block the
/// merge; for differing-arity overloads any differing *shared* position blocks it,
/// because adding only a trailing parameter requires the shorter signature to be a
/// type-prefix of the longer.
fn differing_param_positions<'a>(shapes: &[SigShape<'a>], count: usize, source: &'a str) -> usize {
    (0..count)
        .filter(|&pos| {
            let baseline = param_type_text(&shapes[0], pos, source);
            shapes[1..]
                .iter()
                .any(|s| param_type_text(s, pos, source) != baseline)
        })
        .count()
}

/// Whether the signatures form an "overloaded narrowing" group: every signature
/// has a *distinct* parameter list *and* a *distinct* return type, so each
/// parameter shape maps to its own narrowed return (the
/// `(p: Pinia): Pinia` / `(p: undefined): undefined` / `(p: Pinia | undefined):
/// Pinia | undefined` idiom). Unifying them into one union-parameter signature
/// would collapse the per-variant return into the union and erase the
/// narrowing, so such a group is not a smell.
fn signatures_narrow_return_type<'a>(shapes: &[SigShape<'a>], source: &'a str) -> bool {
    if shapes.len() < 2 {
        return false;
    }
    let mut params = FxHashSet::default();
    let mut returns = FxHashSet::default();
    for shape in shapes {
        let Some(return_type) = return_type_text(shape, source) else {
            return false;
        };
        if !params.insert(params_text(shape, source)) || !returns.insert(return_type) {
            return false;
        }
    }
    true
}

/// Whether every signature in the group declares the same return type text (all
/// annotated with equal text, or all unannotated).
fn all_returns_identical<'a>(shapes: &[SigShape<'a>], source: &'a str) -> bool {
    let first = return_type_text(&shapes[0], source);
    shapes[1..]
        .iter()
        .all(|s| return_type_text(s, source) == first)
}

/// Whether every signature in the group declares the same type-parameter list text
/// (all equal, or all absent). Distinguishes overloads whose parameter lists read
/// identically but whose method-level generic constraints differ.
fn all_type_params_identical<'a>(shapes: &[SigShape<'a>], source: &'a str) -> bool {
    let first = type_params_text(&shapes[0], source);
    shapes[1..]
        .iter()
        .all(|s| type_params_text(s, source) == first)
}

/// Whether a group of overload signatures could be merged into one with a union
/// or optional trailing parameter.
///
/// * Parameter counts may differ by at most one — a larger gap would need more
///   than one optional trailing parameter, which the overloads do not express.
/// * When the counts *do* differ, the unified form has to add an optional
///   trailing parameter, so the declared return types must be identical (otherwise
///   the merge would erase the per-overload return-type distinction — the curried
///   zero-arg vs one-arg overload idiom, and the D3-style getter/setter idiom where
///   the 0-arg getter returns the value and the 1-arg setter returns `this`) *and*
///   every shared parameter position must have identical type text, since adding
///   only a trailing parameter requires the shorter signature to be a type-prefix of
///   the longer; a differing shared position (the kysely `innerJoin(table, k1, k2)`
///   vs `innerJoin(table, callback)` idiom) would force a union at an internal
///   position, over-permitting combinations the overloads reject.
/// * When the counts are equal the merge unions a single parameter's type. This is
///   unsafe — so the group is not unifiable — when the signatures form an
///   overloaded-narrowing group (distinct parameter lists each mapping to a
///   distinct, narrower return), or their return types are not all identical *while*
///   the inputs differ (a differing parameter position or type-parameter
///   constraint), because then each overload narrows the return conditionally on its
///   input — the trpc `subscription<$Output extends AsyncIterable>: SubscriptionProcedure`
///   vs `subscription<$Output extends Observable>: LegacyObservableSubscriptionProcedure`
///   idiom — which a single union return cannot express, or they differ at two or
///   more parameter positions (the EventEmitter idiom, where a string-literal event
///   and its narrowed listener both differ from the catch-all overload; a
///   per-position union would admit listener/event combinations the overloads
///   reject). Overloads with identical inputs but differing returns are a redundant
///   duplicate whose only merge — a union return — is itself the smell, so they
///   still fire.
fn signatures_are_unifiable<'a>(shapes: &[SigShape<'a>], source: &'a str) -> bool {
    let mut counts = shapes.iter().map(|s| s.params.items.len());
    let Some(first) = counts.next() else {
        return true;
    };
    let (mut min, mut max) = (first, first);
    for count in counts {
        min = min.min(count);
        max = max.max(count);
    }
    if max - min > 1 {
        return false;
    }
    if min == max {
        if signatures_narrow_return_type(shapes, source) {
            return false;
        }
        let differing = differing_param_positions(shapes, min, source);
        if !all_returns_identical(shapes, source)
            && (differing > 0 || !all_type_params_identical(shapes, source))
        {
            return false;
        }
        return differing < 2;
    }

    if !all_returns_identical(shapes, source) {
        return false;
    }
    differing_param_positions(shapes, min, source) == 0
}

/// One overload group sharing a name: the source offsets where each signature
/// starts (for diagnostics) and the parameter/return shapes (for unifiability).
#[derive(Default)]
struct SigGroup<'a> {
    offsets: Vec<u32>,
    shapes: Vec<SigShape<'a>>,
}

fn collect_signatures<'a>(
    members: &'a [TSSignature<'a>],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut seen: FxHashMap<String, SigGroup<'a>> = FxHashMap::default();
    let mut call_sigs: Vec<&TSCallSignatureDeclaration<'a>> = Vec::new();

    for sig in members {
        match sig {
            TSSignature::TSCallSignatureDeclaration(call) => {
                let group = seen.entry("[[call]]".to_string()).or_default();
                group.offsets.push(call.span.start);
                group.shapes.push(SigShape {
                    type_params: call.type_parameters.as_deref(),
                    params: &call.params,
                    return_type: call.return_type.as_deref(),
                });
                call_sigs.push(call);
            }
            TSSignature::TSMethodSignature(method) => {
                let name = match &method.key {
                    PropertyKey::StaticIdentifier(id) => id.name.to_string(),
                    PropertyKey::StringLiteral(s) => s.value.to_string(),
                    _ => continue,
                };
                let group = seen.entry(name).or_default();
                group.offsets.push(method.span.start);
                group.shapes.push(SigShape {
                    type_params: method.type_parameters.as_deref(),
                    params: &method.params,
                    return_type: method.return_type.as_deref(),
                });
            }
            _ => {}
        }
    }

    if call_signatures_are_path_discriminated(&call_sigs) {
        seen.remove("[[call]]");
    }

    seen.retain(|_, group| signatures_are_unifiable(&group.shapes, ctx.source));

    for (name, group) in &seen {
        let offsets = &group.offsets;
        if offsets.len() < 2 {
            continue;
        }
        for &offset in &offsets[1..] {
            let display_name = if name == "[[call]]" {
                "Call signatures".to_string()
            } else {
                format!("`{name}` signatures")
            };
            let (line, _column) = byte_offset_to_line_col(ctx.source, offset as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "{display_name} can be unified into a single signature \
                     with a union or optional parameter."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSInterfaceDeclaration, AstType::TSTypeAliasDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::TSInterfaceDeclaration(decl) => {
                collect_signatures(&decl.body.body, ctx, diagnostics);
            }
            AstKind::TSTypeAliasDeclaration(decl) => {
                if let oxc_ast::ast::TSType::TSTypeLiteral(lit) = &decl.type_annotation {
                    collect_signatures(&lit.members, ctx, diagnostics);
                }
            }
            _ => {}
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
    fn flags_duplicate_call_signatures() {
        let diags = run_on("interface Foo {\n  (x: string): void;\n  (x: number): void;\n}");
        assert_eq!(diags.len(), 1);
    }

    // Regression #1088: Azure SDK `path()` routing — each call signature maps a
    // distinct string-literal path to its own return type. Unifying would erase
    // the per-path return-type inference, so these are not unifiable.
    #[test]
    fn allows_path_discriminated_call_signatures() {
        assert!(
            run_on(
                "export interface Routes {\n  \
                 (path: \"/geocode\"): GetGeocoding;\n  \
                 (path: \"/geocode:batch\"): GetGeocodingBatch;\n  \
                 (path: \"/search/polygon\"): GetPolygon;\n  \
                 (path: \"/reverseGeocode\"): GetReverseGeocoding;\n}"
            )
            .is_empty()
        );
    }

    #[test]
    fn flags_call_signatures_with_duplicate_string_literal() {
        let diags = run_on(
            "interface Foo {\n  \
             (path: \"/a\"): X;\n  \
             (path: \"/a\"): Y;\n}",
        );
        assert_eq!(diags.len(), 1);
    }

    // Regression #1977: zustand `Create` — a one-argument signature returning the
    // bound store directly, alongside a zero-argument curried overload returning a
    // factory. Different arity *and* different return types: no single signature
    // (union or optional parameter) expresses both without collapsing the returns.
    #[test]
    fn allows_curried_zero_arg_vs_one_arg_overload() {
        assert!(
            run_on(
                "type Create = {\n  \
                 <T, Mos extends [string, unknown][] = []>(initializer: StateCreator<T, [], Mos>): UseBoundStore<Mutate<StoreApi<T>, Mos>>;\n  \
                 <T>(): <Mos extends [string, unknown][] = []>(initializer: StateCreator<T, [], Mos>) => UseBoundStore<Mutate<StoreApi<T>, Mos>>;\n}"
            )
            .is_empty()
        );
    }

    // Guard: call signatures differing by exactly one trailing optional parameter
    // and sharing the same return type are genuinely unifiable, so still fire.
    #[test]
    fn flags_trailing_optional_parameter_overload() {
        let diags = run_on(
            "interface Foo {\n  \
             (a: number): void;\n  \
             (a: number, b?: string): void;\n}",
        );
        assert_eq!(diags.len(), 1);
    }

    // Regression #1721: pinia `_SetActivePinia` — each call signature maps a
    // distinct parameter type to its own narrowed return type. Unifying them into
    // `(pinia: Pinia | undefined): Pinia | undefined` would erase the per-variant
    // return-type narrowing, so these are not unifiable.
    #[test]
    fn allows_overloaded_narrowing_return_types() {
        assert!(
            run_on(
                "interface _SetActivePinia {\n  \
                 (pinia: Pinia): Pinia\n  \
                 (pinia: undefined): undefined\n  \
                 (pinia: Pinia | undefined): Pinia | undefined\n}"
            )
            .is_empty()
        );
    }

    // Guard: equal param counts with distinct parameter types but the *same*
    // return type are genuinely unifiable into one union-parameter signature, so
    // still fire — the narrowing exemption must not swallow this real smell.
    #[test]
    fn flags_distinct_params_with_shared_return_type() {
        let diags = run_on(
            "interface Foo {\n  \
             (x: string): void;\n  \
             (x: number): void;\n}",
        );
        assert_eq!(diags.len(), 1);
    }

    // Guard: param counts differing by more than one cannot be merged with a
    // single optional trailing parameter, so they are not a smell.
    #[test]
    fn allows_call_signatures_differing_by_more_than_one_param() {
        assert!(
            run_on(
                "interface Foo {\n  \
                 (a: number): void;\n  \
                 (a: number, b: string, c: string): void;\n}"
            )
            .is_empty()
        );
    }

    // Regression #4751: D3-style getter/setter method overloads — a 0-arg getter
    // returning the current value and a 1-arg setter returning `this` for
    // chaining. Different arity *and* different return types: no single signature
    // expresses both without collapsing the per-arity returns into a union.
    #[test]
    fn allows_d3_getter_setter_method_overloads() {
        assert!(
            run_on(
                "export interface Cloud<T extends CloudWord> {\n  \
                 timeInterval(): number;\n  \
                 timeInterval(interval: number): Cloud<T>;\n  \
                 size(): [number, number];\n  \
                 size(size: [number, number]): Cloud<T>;\n  \
                 rotate(): (datum: T, index: number) => number;\n  \
                 rotate(rotate: number | ((datum: T, index: number) => number)): Cloud<T>;\n}"
            )
            .is_empty()
        );
    }

    // Guard: a method overload pair differing only by a single parameter and
    // sharing the same return type is genuinely unifiable, so still fires.
    #[test]
    fn flags_unifiable_method_overloads() {
        let diags = run_on(
            "interface Foo {\n  \
             bar(a: string): void;\n  \
             bar(a: number): void;\n}",
        );
        assert_eq!(diags.len(), 1);
    }

    // Regression #6254: Node.js EventEmitter-style overloads — a string-literal
    // event paired with a narrowed listener, then a catch-all `string` event with a
    // generic listener, both returning `this`. Both parameters differ across the
    // pair, so a per-position union would let `(...args) => void` pair with the
    // `'error'` event, which the overloads reject. Two differing positions => not
    // unifiable.
    #[test]
    fn allows_event_emitter_overloads() {
        assert!(
            run_on(
                "export interface TediousConnection {\n  \
                 off(event: 'error', listener: (error: unknown) => void): this\n  \
                 off(event: string, listener: (...args: any[]) => void): this\n  \
                 on(event: 'error', listener: (error: unknown) => void): this\n  \
                 on(event: string, listener: (...args: any[]) => void): this\n  \
                 once(event: 'end', listener: () => void): this\n  \
                 once(event: string, listener: (...args: any[]) => void): this\n}"
            )
            .is_empty()
        );
    }

    // Guard: an equal-arity pair differing at two parameter positions cannot be
    // merged without a per-position union that admits `f(number, number)` and
    // `f(string, string)`, which neither overload declares. Not unifiable.
    #[test]
    fn allows_two_position_differing_overloads() {
        assert!(
            run_on(
                "interface Foo {\n  \
                 f(a: number, b: string): void;\n  \
                 f(a: string, b: number): void;\n}"
            )
            .is_empty()
        );
    }

    // Guard: a single differing parameter position with a shared return type is
    // genuinely unifiable into `x: number | string`, so still fires.
    #[test]
    fn flags_single_position_differing_overloads() {
        let diags = run_on(
            "interface Foo {\n  \
             f(x: number): void;\n  \
             f(x: string): void;\n}",
        );
        assert_eq!(diags.len(), 1);
    }

    // Guard: a string-literal vs `string` first parameter where the listener type is
    // *identical* differs at exactly one position, so the literal overload is
    // subsumed and the pair is unifiable — must still fire (distinguishes the fix
    // from a naive "literal-vs-string first param => exempt" carve-out).
    #[test]
    fn flags_literal_vs_string_with_identical_remaining_param() {
        let diags = run_on(
            "interface Foo {\n  \
             off(event: 'error', listener: (e: unknown) => void): this\n  \
             off(event: string, listener: (e: unknown) => void): this\n}",
        );
        assert_eq!(diags.len(), 1);
    }

    // Regression #6255: kysely `innerJoin` — a 3-param `(table, k1, k2)` overload and
    // a 2-param `(table, callback)` overload sharing the same return type. The counts
    // differ by one, but shared position 1 differs (`K1` vs `FN`), so the shorter
    // signature is not a type-prefix of the longer: adding only a trailing optional
    // parameter cannot express both without a union at an internal position that
    // over-permits. Not unifiable.
    #[test]
    fn allows_diff_one_overloads_with_differing_shared_position() {
        assert!(
            run_on(
                "interface SelectQueryBuilder<DB, TB, O> {\n  \
                 innerJoin<TE extends TableExpression<DB, TB>, K1 extends JoinReferenceExpression<DB, TB, TE>, K2 extends JoinReferenceExpression<DB, TB, TE>>(table: TE, k1: K1, k2: K2): SelectQueryBuilderWithInnerJoin<DB, TB, O, TE>\n  \
                 innerJoin<TE extends TableExpression<DB, TB>, const FN extends JoinCallbackExpression<DB, TB, TE>>(table: TE, callback: FN): SelectQueryBuilderWithInnerJoin<DB, TB, O, TE>\n}"
            )
            .is_empty()
        );
    }

    // Regression #7117 (Case 1): trpc `subscription` — two equal-arity method
    // overloads with textually-identical parameters but different method-level
    // generic constraints and different return types. The params compare as 0
    // differing positions, but the returns differ, so each overload narrows its
    // return conditionally on `$Output`'s constraint — a single union-parameter
    // signature cannot express that. Not unifiable.
    #[test]
    fn allows_equal_arity_overloads_with_differing_return_types() {
        assert!(
            run_on(
                "interface ProcedureBuilder<TContext> {\n  \
                 subscription<$Output extends AsyncIterable<any, void, any>>(resolver: ProcedureResolver<TContext, $Output>): SubscriptionProcedure<$Output>;\n  \
                 subscription<$Output extends Observable<any, any>>(resolver: ProcedureResolver<TContext, $Output>): LegacyObservableSubscriptionProcedure<$Output>;\n}"
            )
            .is_empty()
        );
    }

    // Regression #7117 (Case 2): trpc `useTRPCInfiniteQuery` — three equal-arity
    // call signatures whose returns are mixed (overload 1 returns
    // `DefinedUseInfiniteQueryResult`, overloads 2–3 share `UseInfiniteQueryResult`).
    // The returns are neither all-distinct (so the narrowing exemption does not
    // apply) nor all-identical, so the group conditionally narrows the return on the
    // `opts` shape, which a union `opts` parameter cannot replicate. Not unifiable.
    #[test]
    fn allows_equal_arity_call_signatures_with_mixed_return_types() {
        assert!(
            run_on(
                "interface useTRPCInfiniteQuery<TDef> {\n  \
                 <TData>(input: TInput, opts: DefinedInitialDataInfiniteOptions<TData>): DefinedUseInfiniteQueryResult<TData>;\n  \
                 <TData>(input: TInput, opts?: UndefinedInitialDataInfiniteOptions<TData>): UseInfiniteQueryResult<TData>;\n  \
                 <TData>(input: TInput, opts?: UseInfiniteQueryOptions<TData>): UseInfiniteQueryResult<TData>;\n}"
            )
            .is_empty()
        );
    }
}
