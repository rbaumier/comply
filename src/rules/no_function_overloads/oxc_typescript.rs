use rustc_hash::FxHashMap;

use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::{byte_offset_to_line_col, type_annotation_is_type_predicate};
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{Declaration, Function, Statement};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Word-boundary-aware search for `needle` within `source[start..end]`.
fn source_contains_ident(source: &str, start: u32, end: u32, needle: &str) -> bool {
    let Some(slice) = source.get(start as usize..end as usize) else { return false; };
    let mut search_from = 0;
    while let Some(pos) = slice[search_from..].find(needle) {
        let abs = search_from + pos;
        let before_ok = abs == 0
            || !slice.as_bytes()[abs - 1].is_ascii_alphanumeric()
                && slice.as_bytes()[abs - 1] != b'_';
        let after_pos = abs + needle.len();
        let after_ok = after_pos >= slice.len()
            || !slice.as_bytes()[after_pos].is_ascii_alphanumeric()
                && slice.as_bytes()[after_pos] != b'_';
        if before_ok && after_ok {
            return true;
        }
        search_from = abs + 1;
    }
    false
}

/// Generic parameter names that appear in this overload signature's return type.
/// Returns `None` when the signature has no generics or no return-type annotation
/// (i.e. cannot be load-bearing for return-type inference).
fn generics_in_return_type(source: &str, f: &Function) -> Option<Vec<String>> {
    let type_params = f.type_parameters.as_deref()?;
    let return_type = f.return_type.as_ref()?;
    let mut names = Vec::new();
    for tp in &type_params.params {
        let name = tp.name.name.as_str();
        if source_contains_ident(source, return_type.span.start, return_type.span.end, name) {
            names.push(name.to_string());
        }
    }
    if names.is_empty() { None } else { Some(names) }
}

/// True when the signature's return type is a `x is T` type predicate. Such
/// overloads narrow the return type per input variant and cannot collapse into
/// a single union signature without erasing that narrowing at every call site.
fn returns_type_predicate(f: &Function) -> bool {
    type_annotation_is_type_predicate(f.return_type.as_deref())
}

/// The source text of the signature's return-type annotation, normalized of
/// surrounding whitespace. `None` when the signature has no return annotation.
fn return_type_text(source: &str, f: &Function) -> Option<String> {
    let return_type = f.return_type.as_ref()?;
    source
        .get(return_type.span.start as usize..return_type.span.end as usize)
        .map(|s| s.trim().to_string())
}

/// The source text of each fixed parameter's type annotation, positionally.
/// A parameter without a type annotation yields an empty string so positions
/// still align across signatures. The trailing rest parameter, when present, is
/// not included — it is tracked separately via `first_param_is_rest`/arity.
fn param_type_texts(source: &str, f: &Function) -> Vec<String> {
    f.params
        .items
        .iter()
        .map(|param| {
            param
                .type_annotation
                .as_ref()
                .and_then(|ann| {
                    source.get(
                        ann.type_annotation.span().start as usize
                            ..ann.type_annotation.span().end as usize,
                    )
                })
                .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
                .unwrap_or_default()
        })
        .collect()
}

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            if let AstKind::Program(program) = node.kind() {
                let mut groups: FxHashMap<String, Vec<OverloadSig>> = FxHashMap::default();
                for stmt in &program.body {
                    if let Some(sig) = extract_overload_sig(ctx.source, stmt) {
                        groups.entry(sig.name.clone()).or_default().push(sig);
                    }
                }
                for (name, sigs) in groups {
                    if sigs.len() < 2 {
                        continue;
                    }
                    if preserves_generic_return_inference(&sigs) {
                        continue;
                    }
                    if preserves_type_predicate_narrowing(&sigs) {
                        continue;
                    }
                    if preserves_call_context_return(&sigs) {
                        continue;
                    }
                    if preserves_rest_vs_fixed_return(&sigs) {
                        continue;
                    }
                    if preserves_non_terminal_optional(&sigs) {
                        continue;
                    }
                    if preserves_concrete_param_discrimination(&sigs) {
                        continue;
                    }
                    for sig in sigs {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, sig.span_start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "Function '{name}' has overload signatures — overloads \
                                 don't constrain the implementation and break inference. \
                                 Use a union parameter type or a generic signature instead."
                            ),
                            severity: super::META.severity,
                            span: None,
                        });
                    }
                }
            }
        }
        diagnostics
    }
}

struct OverloadSig {
    name: String,
    span_start: u32,
    /// Generic parameter names that appear in this signature's return type.
    /// Empty when the signature has no generics referenced in its return type.
    generics_in_return: Vec<String>,
    /// True when this signature returns a `x is T` type predicate.
    returns_predicate: bool,
    /// Number of declared parameters in this signature, counting a trailing rest
    /// parameter as one.
    param_count: usize,
    /// True when this signature's first parameter is a rest (`...args`) parameter.
    first_param_is_rest: bool,
    /// Source text of this signature's return-type annotation, if any.
    return_type: Option<String>,
    /// Source text of each fixed parameter's type annotation, positionally.
    /// Excludes a trailing rest parameter (tracked via arity / `first_param_is_rest`).
    param_types: Vec<String>,
}

/// True when EVERY overload signature has a generic type parameter that appears
/// in its return type. Each such overload's return type is parameterized by its
/// own generics, so it is load-bearing for generic return-type inference at the
/// call site. The generic names need not be shared across signatures: the
/// canonical "optional-parameter overload" pattern uses disjoint generics —
/// `f<S>(api: S): R<S>` and `f<S, U>(api: S, sel: (s) => U): U` infer different
/// return types from different call sites. Collapsing such overloads into one
/// signature would widen the return type via union (`R<S> | U`) and lose that
/// per-call-site inference. A group where some signature lacks a generic in its
/// return type does not qualify: that signature collapses into the union cleanly.
fn preserves_generic_return_inference(sigs: &[OverloadSig]) -> bool {
    sigs.iter().all(|sig| !sig.generics_in_return.is_empty())
}

/// True when EVERY overload signature returns a `x is T` type predicate. Each
/// predicate narrows the return type for its specific input variant; collapsing
/// the overloads into one union signature would erase that per-call-site
/// narrowing and force `as` casts on every caller.
fn preserves_type_predicate_narrowing(sigs: &[OverloadSig]) -> bool {
    sigs.iter().all(|sig| sig.returns_predicate)
}

/// True when the overloads carry at least two distinct return-type annotations.
/// A group that does not vary its return type collapses cleanly into a single
/// signature, so distinct returns are the precondition for every "the return
/// type is load-bearing per variant" exemption below.
fn has_distinct_return_types(sigs: &[OverloadSig]) -> bool {
    let mut return_types = sigs.iter().filter_map(|s| s.return_type.as_deref());
    let Some(first_return) = return_types.next() else {
        return false;
    };
    return_types.any(|r| r != first_return)
}

/// True when the overloads select distinct return types by parameter presence —
/// the call context (e.g. a TC39 `@decorator` vs a plain call) is determined by
/// whether an extra parameter is supplied. Such overloads carry at least two
/// different return-type annotations AND at least two different arities, so a
/// single optional/union parameter cannot reproduce them: collapsing would widen
/// the return to a union (e.g. `T | void`) and break callers that rely on the
/// per-context return type.
fn preserves_call_context_return(sigs: &[OverloadSig]) -> bool {
    if !has_distinct_return_types(sigs) {
        return false;
    }
    let mut arities = sigs.iter().map(|s| s.param_count);
    let first_arity = arities.next().expect("group has at least two signatures");
    arities.any(|a| a != first_arity)
}

/// True when the overloads discriminate on a structurally incompatible first
/// parameter — at least one signature leads with a rest (`...args: T[]`)
/// parameter and at least one leads with a fixed parameter — AND carry distinct
/// return types. The rest-vs-fixed shapes cannot be merged into one parameter
/// without a `T[] | Fixed` union that erases the per-variant return type (e.g.
/// mobx-react `inject`'s rest-strings HOC vs its function-arg HOC), so collapsing
/// the overloads would break inference at call sites.
fn preserves_rest_vs_fixed_return(sigs: &[OverloadSig]) -> bool {
    if !has_distinct_return_types(sigs) {
        return false;
    }
    let any_rest_first = sigs.iter().any(|s| s.first_param_is_rest);
    let any_fixed_first = sigs.iter().any(|s| !s.first_param_is_rest && s.param_count > 0);
    any_rest_first && any_fixed_first
}

/// True when some pair of overloads differs by a parameter inserted in a
/// NON-TERMINAL position — i.e. an arity-`n` signature whose fixed parameter
/// types are not a positional prefix of an arity-`(n+1)` signature, yet the two
/// share their trailing parameter type. Collapsing such a pair would require a
/// middle optional `f(a, b?, c?)`, which TypeScript cannot express so that both
/// `f(a, c)` and `f(a, b, c)` type-check: the second positional argument would be
/// ambiguous between the `b` and `c` types. The separate overloads are therefore
/// required (e.g. TypeORM's `OneToOne(typeFn, options?)` vs
/// `OneToOne(typeFn, inverseSide?, options?)`).
///
/// A pure trailing-optional pair (the shorter list IS a prefix of the longer one)
/// or a same-position type difference is NOT exempted here — those collapse into
/// a single signature cleanly and must still be flagged.
fn preserves_non_terminal_optional(sigs: &[OverloadSig]) -> bool {
    sigs.iter().any(|shorter| {
        sigs.iter().any(|longer| {
            longer.param_types.len() == shorter.param_types.len() + 1
                && extra_param_is_non_terminal(&shorter.param_types, &longer.param_types)
        })
    })
}

/// Given a shorter parameter-type list and a longer one with exactly one extra
/// parameter, return true when that extra parameter sits in a NON-TERMINAL
/// position — the longer list is the shorter list with one type inserted before
/// its end. Detected by finding the insertion index `k`: the first `k` types
/// match positionally and the remaining `shorter[k..]` match `longer[k+1..]`. An
/// insertion at `k == shorter.len()` is a trailing append (returns false); any
/// `k < shorter.len()` is a middle insertion (returns true). Returns false when
/// no single-insertion alignment exists (the lists differ in more than the one
/// inserted slot).
fn extra_param_is_non_terminal(shorter: &[String], longer: &[String]) -> bool {
    for k in 0..shorter.len() {
        let prefix_matches = shorter[..k] == longer[..k];
        let suffix_matches = shorter[k..] == longer[k + 1..];
        if prefix_matches && suffix_matches {
            return true;
        }
    }
    false
}

/// True when the overloads discriminate on the concrete type of a single
/// same-position parameter to narrow the return type per call site — every
/// signature has the same arity and the same fixed-parameter types except in one
/// position, AND the signatures carry at least two distinct return-type
/// annotations. Each overload returns the specific type produced for its specific
/// input type (e.g. graphql-js `typeFromAST(schema, NamedTypeNode)` →
/// `GraphQLNamedType | undefined` vs `(schema, ListTypeNode)` →
/// `GraphQLList<any> | undefined`). Collapsing the discriminating parameter into
/// a union would widen the return to the union of all branches for ALL callers,
/// erasing the per-input narrowing that is the entire point of the overloads.
///
/// A group that varies its parameter type at a shared position but returns the
/// SAME type (e.g. `f(a: string): X; f(a: number): X;`) is NOT exempted here — it
/// collapses cleanly into `f(a: string | number): X` and must still be flagged.
fn preserves_concrete_param_discrimination(sigs: &[OverloadSig]) -> bool {
    if !has_distinct_return_types(sigs) {
        return false;
    }
    let arity = sigs[0].param_types.len();
    if arity == 0 || sigs.iter().any(|s| s.param_types.len() != arity) {
        return false;
    }
    if sigs.iter().any(|s| s.first_param_is_rest) {
        return false;
    }
    // Exactly one parameter position varies across the group; all others are
    // identical in every signature. That single varying position is the
    // discriminant whose concrete type selects the return type.
    let varying_positions = (0..arity)
        .filter(|&i| {
            let first = sigs[0].param_types[i].as_str();
            sigs.iter().any(|s| s.param_types[i] != first)
        })
        .count();
    varying_positions == 1
}

/// Extract overload signature info if `stmt` is a function declaration without a body.
fn extract_overload_sig(source: &str, stmt: &Statement) -> Option<OverloadSig> {
    match stmt {
        Statement::FunctionDeclaration(f) => sig_from_function(source, f),
        Statement::ExportNamedDeclaration(exp) => {
            if let Some(ref decl) = exp.declaration
                && let Declaration::FunctionDeclaration(f) = decl
            {
                return sig_from_function(source, f);
            }
            None
        }
        _ => None,
    }
}

fn sig_from_function(source: &str, f: &Function) -> Option<OverloadSig> {
    if f.body.is_some() {
        return None;
    }
    let name = f.id.as_ref()?.name.to_string();
    let generics_in_return = generics_in_return_type(source, f).unwrap_or_default();
    let has_rest = f.params.rest.is_some();
    let param_count = f.params.items.len() + usize::from(has_rest);
    // The rest parameter, when present, always trails the fixed parameters, so a
    // rest in first position means there are no fixed parameters before it.
    let first_param_is_rest = has_rest && f.params.items.is_empty();
    Some(OverloadSig {
        name,
        span_start: f.span.start,
        generics_in_return,
        returns_predicate: returns_type_predicate(f),
        param_count,
        first_param_is_rest,
        return_type: return_type_text(source, f),
        param_types: param_type_texts(source, f),
    })
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
    fn flags_overloaded_function() {
        // One parameter's type differs but the return type is identical, so the
        // pair collapses cleanly into `foo(x: number | string): string`.
        let source = "
function foo(x: number): string;
function foo(x: string): string;
function foo(x: number | string): string { return String(x); }
";
        assert_eq!(run_on(source).len(), 2);
    }

    #[test]
    fn allows_single_signature() {
        assert!(run_on("function foo(x: number): string { return String(x); }").is_empty());
    }

    #[test]
    fn allows_distinct_functions() {
        let source = "function foo(): void {} function bar(): void {}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_overloads_load_bearing_for_generic_return_inference() {
        // Regression for #109: overloads of `make<...>(opts)` whose generic `C`
        // appears in each return type are load-bearing — collapsing them would
        // widen `sort` to `unknown` via union.
        let source = r#"
type SortColumns = readonly [string, ...string[]];
type SortFor<T extends SortColumns> = `${T[number]}:asc` | `${T[number]}:desc`;
export function make<const C extends SortColumns>(opts: {
  sortColumns: C; defaultSort: SortFor<C>;
}): { sort: SortFor<C> };
export function make<
  F extends Record<string, unknown>,
  const C extends SortColumns,
>(opts: { filters: F; sortColumns: C; defaultSort: SortFor<C> }): { sort: SortFor<C>; filters: F };
export function make<
  F extends Record<string, unknown>,
  const C extends SortColumns,
>(opts: { filters?: F; sortColumns: C; defaultSort: SortFor<C> }) {
  return {} as any;
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_disjoint_generic_return_overloads() {
        // Regression for #1665: zustand `useStore` returns `ExtractState<S>` in
        // its selector-less form and `U` in its selector form. The return-type
        // generics are disjoint (`S` vs `U`), so no shared name exists, yet each
        // overload's return type is inferred per call site. Collapsing into
        // `(api: S, selector?: ...) => ExtractState<S> | U` would erase that
        // inference and hand every caller the widened union.
        let source = r#"
export function useStore<S extends ReadonlyStoreApi<unknown>>(
  api: S,
): ExtractState<S>;
export function useStore<S extends ReadonlyStoreApi<unknown>, U>(
  api: S,
  selector: (state: ExtractState<S>) => U,
): U;
export function useStore<TState, StateSlice>(
  api: ReadonlyStoreApi<TState>,
  selector: (state: TState) => StateSlice = identity as any,
) {
  return {} as any;
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_disjoint_generic_return_overloads_at_same_arity() {
        // The disjoint-generic-return pattern is load-bearing even when the
        // overloads share an arity: the arity-based call-context exemption does
        // not fire here, but each return type is still inferred per call site.
        let source = "
function pick<S extends Api>(api: S): ExtractState<S>;
function pick<S extends Api, U>(api: (state: S) => U): U;
function pick(api: any): any { return api; }
";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_overloads_when_generics_dont_reach_return_type() {
        // Generics exist but only appear in params, not return — not load-bearing.
        let source = "
function foo<T>(x: T): string;
function foo<T>(x: T, y: number): string;
function foo<T>(x: T, y?: number): string { return ''; }
";
        assert_eq!(run_on(source).len(), 2);
    }

    #[test]
    fn allows_type_predicate_overloads() {
        // Regression for #1085: `isUnexpected` overloads each narrow `response`
        // to a specific *Default response via an `is T` predicate. Collapsing
        // them into one union signature would erase per-call-site narrowing.
        let source = r#"
export function isUnexpected(
  response: SearchGetGeocoding200Response | SearchGetGeocodingDefaultResponse,
): response is SearchGetGeocodingDefaultResponse;
export function isUnexpected(
  response: SearchGetGeocodingBatch200Response | SearchGetGeocodingBatchDefaultResponse,
): response is SearchGetGeocodingBatchDefaultResponse;
export function isUnexpected(response: AllResponses): response is AllResponses {
  return false;
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_overloads_when_only_some_return_predicate() {
        // One overload returns a type predicate, the other returns a plain type
        // — the group is not uniformly predicate-narrowing. Both params vary, so
        // it is not single-parameter concrete discrimination either; it collapses
        // into one union signature and still fires.
        let source = "
function foo(x: number, y: string): x is number;
function foo(x: string, y: number): string;
function foo(x: number | string, y: string | number): boolean { return true; }
";
        assert_eq!(run_on(source).len(), 2);
    }

    #[test]
    fn flags_overloads_when_only_some_share_return_generic() {
        // One overload uses T in return, the other doesn't — intersection empty,
        // so the exception does not apply.
        let source = "
function foo<T>(x: T): T;
function foo<T>(x: T): string;
function foo<T>(x: T): any { return x; }
";
        assert_eq!(run_on(source).len(), 2);
    }

    #[test]
    fn allows_decorator_aware_overloads_with_distinct_return_types() {
        // Regression for #1874: mobx-react `observer` has a decorator form
        // (extra `ClassDecoratorContext` param → `void`) and a function-call
        // form (no extra param → `T`). The return types differ and are selected
        // by parameter presence; a union/optional param would widen the return
        // to `T | void` and break callers that use the wrapped component.
        let source = r#"
export function observer<T extends IReactComponent>(component: T, context: ClassDecoratorContext): void;
export function observer<T extends IReactComponent>(component: T): T;
export function observer<T extends IReactComponent>(component: T, context?: ClassDecoratorContext): T {
  return component;
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_rest_vs_fixed_first_param_with_distinct_return_types() {
        // Regression for #1875: mobx-react `inject` has a rest-strings form
        // (`...stores: Array<string>` → a conditional-type HOC) and a function-arg
        // form (`fn: IStoresToProps<...>` → `IWrappedComponent<P>` HOC). The first
        // params are structurally incompatible (rest vs fixed) and the return types
        // are distinct generics; a `string | IStoresToProps` union param would
        // collapse the two return shapes into one less-precise type, breaking
        // per-variant inference at call sites.
        let source = r#"
export function inject(
    ...stores: Array<string>
): <T extends IReactComponent<any>>(
    target: T
) => T & (T extends IReactComponent<infer P> ? IWrappedComponent<P> : never);
export function inject<S extends IValueMap = {}, P extends IValueMap = {}, I extends IValueMap = {}, C extends IValueMap = {}>(
    fn: IStoresToProps<S, P, I, C>
): <T extends IReactComponent>(target: T) => T & IWrappedComponent<P>;
export function inject(...storeNames: Array<any>) {
    return {} as any;
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_rest_vs_fixed_first_param_with_same_return_type() {
        // Same rest-vs-fixed first-param discriminant but an identical return type
        // collapses to one `string[] | Fn` union signature — no per-variant return
        // inference to preserve, so it still fires.
        let source = "
function foo(...names: Array<string>): void;
function foo(fn: () => void): void;
function foo(...args: Array<any>): void {}
";
        assert_eq!(run_on(source).len(), 2);
    }

    #[test]
    fn allows_distinct_return_types_at_same_arity() {
        // Regression for #2400: same arity (one param each) where the single
        // parameter's concrete type selects a distinct return type per call site
        // is genuine discrimination — `foo(number)` infers `string` and
        // `foo(string)` infers `number`. Collapsing to
        // `foo(x: number | string): string | number` would hand every caller the
        // widened return union, erasing the per-input narrowing.
        let source = "
function foo(x: number): string;
function foo(x: string): number;
function foo(x: number | string): string | number { return x as any; }
";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_arity_discriminated_overloads_with_distinct_concrete_return_types() {
        // Regression for #1876: mobx `when` returns a `Promise<void> & { cancel() }`
        // in its 2-param async form and an `IReactionDisposer` in its 3-param
        // callback form. The return types are concrete (no generics) and selected
        // by argument count; a union return would force every caller to narrow, so
        // the overloads are load-bearing and must not collapse.
        let source = r#"
export function when(
    predicate: () => boolean,
    opts?: IWhenOptions
): Promise<void> & { cancel(): void };
export function when(
    predicate: () => boolean,
    effect: Lambda,
    opts?: IWhenOptions
): IReactionDisposer;
export function when(predicate: any, arg1?: any, arg2?: any): any {
    return arg1;
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_distinct_arity_with_same_return_type() {
        // Differing arity but identical return type collapses to one optional
        // param — no per-overload return inference to preserve, so it fires.
        let source = "
function foo<T>(x: T): string;
function foo<T>(x: T, y: number): string;
function foo<T>(x: T, y?: number): string { return ''; }
";
        assert_eq!(run_on(source).len(), 2);
    }

    #[test]
    fn allows_overloads_differing_by_a_non_terminal_optional_param() {
        // Regression for #2369: TypeORM's `OneToOne` decorator factory exposes a
        // 2-arity form `(typeFn, options?)` and a 3-arity form
        // `(typeFn, inverseSide?, options?)`. The shorter list is not a positional
        // prefix of the longer one — the extra `inverseSide` is inserted in the
        // middle, shifting `options` to the end. Unifying would require a middle
        // optional `f(a, b?, c?)`, which TypeScript cannot express such that both
        // `f(a, options)` and `f(a, inverseSide, options)` type-check, so the
        // separate overloads are required.
        let source = r#"
export function OneToOne<T>(
    typeFunctionOrTarget: string | ((type?: any) => ObjectType<T>),
    options?: RelationOptions,
): PropertyDecorator;
export function OneToOne<T>(
    typeFunctionOrTarget: string | ((type?: any) => ObjectType<T>),
    inverseSide?: string | ((object: T) => any),
    options?: RelationOptions,
): PropertyDecorator;
export function OneToOne<T>(
    typeFunctionOrTarget: string | ((type?: any) => ObjectType<T>),
    inverseSideOrOptions?: string | ((object: T) => any) | RelationOptions,
    maybeOptions?: RelationOptions,
): PropertyDecorator {
    return {} as any;
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_trailing_optional_overloads() {
        // A genuine trailing-optional pair: the shorter list IS a positional
        // prefix of the longer one, so it collapses cleanly into
        // `foo(a: string, b?: number): void`. Still flagged.
        let source = "
function foo(a: string): void;
function foo(a: string, b: number): void;
function foo(a: string, b?: number): void {}
";
        assert_eq!(run_on(source).len(), 2);
    }

    #[test]
    fn flags_same_position_type_difference_overloads() {
        // A pair differing only by one parameter's type at the same position
        // but with the SAME return type collapses into
        // `foo(a: string | number): void`. Still flagged.
        let source = "
function foo(a: string): void;
function foo(a: number): void;
function foo(a: string | number): void {}
";
        assert_eq!(run_on(source).len(), 2);
    }

    #[test]
    fn allows_concrete_param_discrimination_with_distinct_return_types() {
        // Regression for #2400: graphql-js `typeFromAST` discriminates on the
        // concrete type of one same-position parameter to narrow the return type
        // per call site. Each overload returns the specific type produced for its
        // specific input type; collapsing the parameter into a union would widen
        // the return to `GraphQLNamedType | GraphQLList<any> | ... | undefined`
        // for ALL callers, erasing the per-input narrowing that is the point.
        let source = r#"
export function typeFromAST(
  schema: GraphQLSchema,
  typeNode: NamedTypeNode,
): GraphQLNamedType | undefined;
export function typeFromAST(
  schema: GraphQLSchema,
  typeNode: ListTypeNode,
): GraphQLList<any> | undefined;
export function typeFromAST(
  schema: GraphQLSchema,
  typeNode: NonNullTypeNode,
): GraphQLNonNull<any> | undefined;
export function typeFromAST(
  schema: GraphQLSchema,
  typeNode: TypeNode,
): GraphQLNamedType | GraphQLList<any> | GraphQLNonNull<any> | undefined {
  return undefined;
}
"#;
        assert!(run_on(source).is_empty());
    }
}
