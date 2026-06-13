use std::collections::HashMap;

use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::{byte_offset_to_line_col, type_annotation_is_type_predicate};
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{Declaration, Function, Statement};
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

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            if let AstKind::Program(program) = node.kind() {
                let mut groups: HashMap<String, Vec<OverloadSig>> = HashMap::new();
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
}

/// True when ALL overload signatures share at least one generic type parameter
/// name that appears in their return type. This signals "overloads load-bearing
/// for generic return-type inference" — collapsing them would widen the return
/// type via union and lose narrow inference at call sites.
fn preserves_generic_return_inference(sigs: &[OverloadSig]) -> bool {
    let mut iter = sigs.iter();
    let Some(first) = iter.next() else {
        return false;
    };
    if first.generics_in_return.is_empty() {
        return false;
    }
    let mut intersection: Vec<String> = first.generics_in_return.clone();
    for sig in iter {
        intersection.retain(|n| sig.generics_in_return.iter().any(|m| m == n));
        if intersection.is_empty() {
            return false;
        }
    }
    true
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
        let source = "
function foo(x: number): string;
function foo(x: string): number;
function foo(x: number | string): string | number { return x as any; }
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
        // — the group is not uniformly predicate-narrowing, so it still fires.
        let source = "
function foo(x: number): x is number;
function foo(x: string): string;
function foo(x: number | string): boolean { return true; }
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
    fn flags_distinct_return_types_at_same_arity() {
        // Same arity (one param each) with different return types is collapsible
        // into `foo(x: number | string): string | number` — not call-context
        // selected, so it still fires.
        let source = "
function foo(x: number): string;
function foo(x: string): number;
function foo(x: number | string): string | number { return x as any; }
";
        assert_eq!(run_on(source).len(), 2);
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
}
