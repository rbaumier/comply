use rustc_hash::FxHashMap;

use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::{byte_offset_to_line_col, type_annotation_is_type_predicate};
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Declaration, Function, Statement, TSType, TSTypeName, TSTypeOperatorOperator,
};
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

/// True when `ty` is the tagged-template marker type — the lib type
/// `TemplateStringsArray`, or its structural equivalent `readonly string[]`. A
/// function is callable with tagged-template syntax (`` tag`...` ``) only when
/// its first parameter accepts that readonly string array, so an overload that
/// leads with it cannot be replaced by a union parameter. A mutable `string[]`
/// does NOT qualify: `TemplateStringsArray` is readonly and is not assignable to
/// a mutable array, so such a parameter is not tag-callable.
fn type_is_tagged_template_marker(ty: &TSType) -> bool {
    match ty {
        TSType::TSTypeReference(tref) => matches!(
            &tref.type_name,
            TSTypeName::IdentifierReference(id) if id.name.as_str() == "TemplateStringsArray"
        ),
        TSType::TSTypeOperatorType(op) if op.operator == TSTypeOperatorOperator::Readonly => {
            matches!(
                &op.type_annotation,
                TSType::TSArrayType(arr) if matches!(&arr.element_type, TSType::TSStringKeyword(_))
            )
        }
        _ => false,
    }
}

/// True when the signature's first formal parameter is annotated with the
/// tagged-template marker type ([`type_is_tagged_template_marker`]).
fn first_param_is_tagged_template(f: &Function) -> bool {
    f.params
        .items
        .first()
        .and_then(|param| param.type_annotation.as_ref())
        .is_some_and(|ann| type_is_tagged_template_marker(&ann.type_annotation))
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

/// When `s` is exactly a single generic application `Name<...>` — the angle
/// bracket pair opened by the first `<` closes at the final character of `s` —
/// return the text between those outermost angle brackets. Returns `None` when
/// `s` has no `<`, or when the matching `>` is not `s`'s last character (e.g. a
/// union such as `GraphQLList<any> | undefined`), i.e. `s` is not a lone
/// generic application.
fn strip_outer_generic(s: &str) -> Option<&str> {
    let open = s.find('<')?;
    let mut depth = 0usize;
    let mut close = None;
    for (i, &b) in s.as_bytes().iter().enumerate().skip(open) {
        match b {
            b'<' => depth += 1,
            b'>' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;
    (close == s.len() - 1).then(|| s[open + 1..close].trim())
}

/// The whitespace-normalized text of `s`'s outermost generic argument list when
/// `s` is a single `Wrapper<...>` application (e.g. `MaybeRefOrGetter<string>` →
/// `string`, `UseStorageOptions<T>` → `T`, `RemovableRef<T>` → `T`,
/// `Map<string, number>` → `string, number`). When `s` has no such wrapper, the
/// whole annotation text is returned. Normalizing lets a parameter's argument
/// list compare positionally against a return type's argument list.
fn type_argument_text(s: &str) -> String {
    let trimmed = s.trim();
    strip_outer_generic(trimmed)
        .unwrap_or(trimmed)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
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
                    if preserves_tagged_template_call(&sigs) {
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
                    if preserves_pairwise_correlated_params(&sigs) {
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
    /// True when this signature's first parameter is the tagged-template marker
    /// type (`TemplateStringsArray` or `readonly string[]`).
    first_param_is_tagged_template: bool,
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

/// True when ANY overload signature leads with the tagged-template marker type
/// (`TemplateStringsArray`, or its structural `readonly string[]` equivalent) as
/// its first parameter. Such a signature is the dedicated
/// tagged-template overload — TypeScript only permits `` tag`...` `` syntax when
/// the function's first parameter has this shape. A union parameter
/// (`TemplateStringsArray | string`) does NOT enable the tag form, so the group
/// cannot collapse and must not be flagged. The quantifier is existential: the
/// tagged-template overload is one specific member whose presence makes the
/// whole group non-collapsible.
fn preserves_tagged_template_call(sigs: &[OverloadSig]) -> bool {
    sigs.iter().any(|sig| sig.first_param_is_tagged_template)
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

/// True when the overloads discriminate on the concrete type of their varying
/// parameter(s) to narrow the return type per call site, with the same arity, no
/// leading rest parameter, and at least two distinct return-type annotations.
///
/// Two shapes qualify:
///
/// - **Single discriminant** — exactly one parameter position varies; its
///   concrete type selects the return type (e.g. graphql-js
///   `typeFromAST(schema, NamedTypeNode)` → `GraphQLNamedType | undefined` vs
///   `(schema, ListTypeNode)` → `GraphQLList<any> | undefined`).
///
/// - **Correlated multi-position discriminant** — several parameter positions
///   vary together because one type argument propagates through them, and at
///   least one of those positions' type argument tracks the return type's type
///   argument call-site by call-site (see [`some_varying_position_tracks_return`],
///   e.g. vueuse `useStorage`, where `defaults` and `options` both carry the
///   stored value type and the return is `RemovableRef<T>`).
///
/// In both shapes, collapsing the discriminating parameter(s) into a union would
/// widen the return to the union of all branches for ALL callers, erasing the
/// per-input narrowing that is the entire point of the overloads.
///
/// A group that varies its parameter type but returns the SAME type (e.g.
/// `f(a: string): X; f(a: number): X;`) is NOT exempted — it collapses cleanly
/// into `f(a: string | number): X` and must still be flagged. Likewise, several
/// positions varying without any of them tracking the return type collapses into
/// independent unions and is still flagged.
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
    let varying_positions: Vec<usize> = (0..arity)
        .filter(|&i| {
            let first = sigs[0].param_types[i].as_str();
            sigs.iter().any(|s| s.param_types[i] != first)
        })
        .collect();
    // A single varying position is the discriminant whose concrete type selects
    // the return type.
    if varying_positions.len() == 1 {
        return true;
    }
    // Several positions vary; they are load-bearing only when their variation is
    // correlated with the return type.
    some_varying_position_tracks_return(sigs, &varying_positions)
}

/// True when some varying parameter position's per-overload type argument is
/// identical, signature by signature, to the return type's per-overload type
/// argument — i.e. that parameter's type argument determines the return type's
/// type argument at every call site. When several parameter positions vary
/// because one type argument propagates through them (e.g. vueuse `useStorage`'s
/// `defaults: MaybeRefOrGetter<T>` and `options: UseStorageOptions<T>` with
/// return `RemovableRef<T>`), collapsing the overloads into a single signature
/// would sever that parameter↔return link and widen the return to a union for
/// every caller, so the overloads are load-bearing. Returns false when any
/// signature lacks a return-type annotation, or when no varying position tracks
/// the return type (the positions vary independently and collapse cleanly).
fn some_varying_position_tracks_return(sigs: &[OverloadSig], varying_positions: &[usize]) -> bool {
    let Some(return_args) = sigs
        .iter()
        .map(|s| s.return_type.as_deref().map(type_argument_text))
        .collect::<Option<Vec<String>>>()
    else {
        return false;
    };
    varying_positions.iter().any(|&pos| {
        sigs.iter()
            .zip(&return_args)
            .all(|(sig, ret_arg)| type_argument_text(&sig.param_types[pos]) == *ret_arg)
    })
}

/// True when the overloads form a SPARSE parameter-type correlation table that a
/// single union signature cannot express. All overloads share the same fixed
/// arity (no leading rest parameter), two or more parameter positions vary across
/// the group, each overload binds a unique combination of types at those varying
/// positions, and those combinations cover only a STRICT subset of the cartesian
/// product of the per-position type sets. The strict subset is the load-bearing
/// signal: the overloads pair specific types together (e.g. valtio
/// `unstable_replaceInternalFunction`, where `name: 'objectIs'` must pair with
/// `fn: (prev: typeof objectIs) => typeof objectIs`, covering 5 of the 25 possible
/// `name`×`fn` combinations). Collapsing the varying positions into independent
/// unions would admit the absent combinations (`name: 'objectIs'` with the
/// `'newProxy'` fn), so the overloads are required regardless of the return type.
///
/// A group where only ONE position varies (`f(x: string): R; f(x: number): R;`)
/// is NOT exempted — it collapses cleanly into `f(x: string | number): R`.
/// Neither is a group whose combinations fill the FULL cartesian product (e.g.
/// `f('x','on'); f('x','off'); f('y','on'); f('y','off')`), which is exactly
/// `f('x' | 'y', 'on' | 'off')`: such groups still flag.
///
/// Type equality reuses the whitespace-normalized `param_types` strings, the same
/// notion of "same type" the other exemptions compare on.
fn preserves_pairwise_correlated_params(sigs: &[OverloadSig]) -> bool {
    let arity = sigs[0].param_types.len();
    if sigs.iter().any(|s| s.param_types.len() != arity) {
        return false;
    }
    if sigs.iter().any(|s| s.first_param_is_rest) {
        return false;
    }
    let varying_positions: Vec<usize> = (0..arity)
        .filter(|&i| {
            let first = sigs[0].param_types[i].as_str();
            sigs.iter().any(|s| s.param_types[i] != first)
        })
        .collect();
    if varying_positions.len() < 2 {
        return false;
    }
    // Each overload's tuple of varying-position types must be unique: a repeated
    // tuple is a duplicate signature that collapses away rather than encoding a
    // distinct correlation.
    let mut combinations: Vec<Vec<&str>> = Vec::with_capacity(sigs.len());
    for sig in sigs {
        let combo: Vec<&str> = varying_positions
            .iter()
            .map(|&pos| sig.param_types[pos].as_str())
            .collect();
        if combinations.contains(&combo) {
            return false;
        }
        combinations.push(combo);
    }
    // The combinations must be a STRICT subset of the cartesian product of the
    // per-position type sets. When they fill the whole product the overloads are
    // exactly `f(p0a | p0b, p1a | p1b)` and collapse cleanly.
    let mut product: usize = 1;
    for &pos in &varying_positions {
        let mut distinct: Vec<&str> = Vec::new();
        for sig in sigs {
            let ty = sig.param_types[pos].as_str();
            if !distinct.contains(&ty) {
                distinct.push(ty);
            }
        }
        product = product.saturating_mul(distinct.len());
    }
    combinations.len() < product
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
        first_param_is_tagged_template: first_param_is_tagged_template(f),
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
    fn allows_mixed_predicate_overloads_with_correlated_params() {
        // Only one overload returns a type predicate, so the uniform-predicate
        // exemption does not fire. But the two parameter positions form a sparse
        // correlation table — the pairs `(number, string)` and `(string, number)`,
        // 2 of the 4 possible combinations — so collapsing into
        // `foo(x: number | string, y: string | number)` would admit
        // `foo(number, number)` and erase the predicate narrowing. The overloads
        // are load-bearing and must not flag.
        let source = "
function foo(x: number, y: string): x is number;
function foo(x: string, y: number): string;
function foo(x: number | string, y: string | number): boolean { return true; }
";
        assert!(run_on(source).is_empty());
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

    #[test]
    fn allows_correlated_multi_position_discrimination_tracking_return() {
        // Regression for #6133: vueuse `useStorage` discriminates on the concrete
        // type of `defaults`, which propagates the same type argument to `options`
        // (`UseStorageOptions<T>`) and to the return (`RemovableRef<T>`). Two
        // parameter positions vary, but the `options` argument tracks the return
        // type argument call-site by call-site, so the overloads are load-bearing:
        // collapsing them would widen the return to
        // `RemovableRef<string | boolean | number | T>` for every caller and
        // sever the per-call-site narrowing.
        let source = r#"
export function useStorage(
  key: MaybeRefOrGetter<string>,
  defaults: MaybeRefOrGetter<string>,
  storage?: StorageLike,
  options?: UseStorageOptions<string>,
): RemovableRef<string>;
export function useStorage(
  key: MaybeRefOrGetter<string>,
  defaults: MaybeRefOrGetter<boolean>,
  storage?: StorageLike,
  options?: UseStorageOptions<boolean>,
): RemovableRef<boolean>;
export function useStorage(
  key: MaybeRefOrGetter<string>,
  defaults: MaybeRefOrGetter<number>,
  storage?: StorageLike,
  options?: UseStorageOptions<number>,
): RemovableRef<number>;
export function useStorage<T>(
  key: MaybeRefOrGetter<string>,
  defaults: MaybeRefOrGetter<T>,
  storage?: StorageLike,
  options?: UseStorageOptions<T>,
): RemovableRef<T>;
export function useStorage<T = unknown>(
  key: MaybeRefOrGetter<string>,
  defaults: MaybeRefOrGetter<null>,
  storage?: StorageLike,
  options?: UseStorageOptions<T>,
): RemovableRef<T>;
export function useStorage<T extends string | number | boolean | object | null>(
  key: MaybeRefOrGetter<string>,
  defaults: MaybeRefOrGetter<T>,
  storage?: StorageLike,
  options?: UseStorageOptions<T>,
): RemovableRef<T> {
  return {} as any;
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_pairwise_correlated_params_with_distinct_return_types() {
        // Two positions vary together as a sparse correlation table — only the
        // pairs `(number, string)` and `(string, number)` exist, 2 of the 4
        // possible combinations. Collapsing into `foo(a: number | string,
        // b: string | number)` would admit `foo(number, number)`, which the
        // overloads forbid, so they are load-bearing regardless of return type.
        let source = "
function foo(a: number, b: string): Date;
function foo(a: string, b: number): RegExp;
function foo(a: any, b: any): any { return a; }
";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_pairwise_correlated_params_when_return_tracking_breaks() {
        // Three overloads pair `Ref<X>` with `Opt<X>` for `X` in
        // `{string, number, boolean}` — 3 of the 9 possible combinations. No
        // varying position tracks the return type, but the pairs are a strict
        // subset of the cartesian product, so a union signature would admit the
        // forbidden cross-combinations; the overloads are load-bearing.
        let source = "
function box(a: Ref<string>, b: Opt<string>): Out<string>;
function box(a: Ref<number>, b: Opt<number>): Out<number>;
function box(a: Ref<boolean>, b: Opt<boolean>): Out<bigint>;
function box(a: any, b: any): any { return a; }
";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_pairwise_correlated_void_overloads() {
        // Regression for #6290: valtio `unstable_replaceInternalFunction` binds
        // each `name` string literal to a specific `fn` type and returns `void`
        // for every overload. Two positions co-vary as a sparse correlation table
        // (5 of 25 `name`×`fn` combinations); collapsing into
        // `f(name: 'objectIs' | ..., fn: FnA | ...)` would let `name: 'objectIs'`
        // pair with the `'newProxy'` fn, breaking the per-name type safety.
        let source = r#"
export function unstable_replaceInternalFunction(
  name: 'objectIs',
  fn: (prev: typeof objectIs) => typeof objectIs,
): void;
export function unstable_replaceInternalFunction(
  name: 'newProxy',
  fn: (prev: typeof newProxy) => typeof newProxy,
): void;
export function unstable_replaceInternalFunction(
  name: 'canProxy',
  fn: (prev: typeof canProxy) => typeof canProxy,
): void;
export function unstable_replaceInternalFunction(
  name: 'createSnapshot',
  fn: (prev: typeof createSnapshot) => typeof createSnapshot,
): void;
export function unstable_replaceInternalFunction(
  name: 'createHandler',
  fn: (prev: typeof createHandler) => typeof createHandler,
): void;
export function unstable_replaceInternalFunction(
  name: 'objectIs' | 'newProxy' | 'canProxy' | 'createSnapshot' | 'createHandler',
  fn: (prev: any) => any,
) {}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_minimal_two_position_correlated_void_pair() {
        // A minimal sparse correlation table returning `void`: two positions vary,
        // only the diagonal pairs exist (2 of 4), so the overloads cannot collapse
        // into independent unions without admitting the off-diagonal calls.
        let source = "
function f(name: 'a', fn: (x: A) => A): void;
function f(name: 'b', fn: (x: B) => B): void;
function f(name: 'a' | 'b', fn: (x: any) => any): void {}
";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_single_varying_position_collapsible_to_union() {
        // Negative control: only ONE parameter position varies, so the group is
        // not a multi-position correlation table — it collapses cleanly into
        // `f(x: string | number): void` and must still flag.
        let source = "
function f(x: string): void;
function f(x: number): void;
function f(x: string | number): void {}
";
        assert_eq!(run_on(source).len(), 2);
    }

    #[test]
    fn flags_duplicated_varying_combination() {
        // Two overloads bind the IDENTICAL pair `('x', number)` and differ only by
        // return type, so the group is not a clean correlation table — the repeated
        // combination is a redundant signature. The duplicate-combination guard
        // declines the sparse-correlation exemption, so the group still flags.
        let source = "
function f(a: 'x', b: number): R1;
function f(a: 'x', b: number): R2;
function f(a: 'y', b: string): R3;
function f(a: any, b: any): any { return a; }
";
        assert_eq!(run_on(source).len(), 3);
    }

    #[test]
    fn allows_tagged_template_overloads() {
        // Regression for #6393: h3's `html` exposes a tagged-template form
        // (`` html`<h1>x</h1>` ``) and a plain-string form (`html("<h1>x</h1>")`).
        // The tagged-template overload MUST lead with `TemplateStringsArray` —
        // a union `TemplateStringsArray | string` does not enable the tag syntax,
        // so the group cannot collapse and must not flag.
        let source = r#"
export function html(strings: TemplateStringsArray, ...values: unknown[]): HTTPResponse;
export function html(markup: string): HTTPResponse;
export function html(first: TemplateStringsArray | string, ...values: unknown[]): HTTPResponse {
  const body =
    typeof first === "string"
      ? first
      : first.reduce((out, str, i) => out + str + (values[i] ?? ""), "");
  return new HTTPResponse(body);
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_tagged_template_overloads_with_string_array_marker() {
        // The structural form of the tag's first arg — `readonly string[]` — is
        // equally a tagged-template marker and exempts the group.
        let source = r#"
function tag(strings: readonly string[], ...values: unknown[]): string;
function tag(markup: string): string;
function tag(first: readonly string[] | string, ...values: unknown[]): string {
  return typeof first === "string" ? first : first.join("");
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_mutable_string_array_first_param_overloads() {
        // A mutable `string[]` first param is NOT tagged-template callable
        // (`TemplateStringsArray` is readonly and not assignable to a mutable
        // array), so the marker exemption must not fire: this group collapses
        // cleanly into `parse(parts: string[] | string): Foo` and still flags.
        let source = "
function parse(parts: string[]): Foo;
function parse(raw: string): Foo;
function parse(input: string[] | string): Foo { return input as any; }
";
        assert_eq!(run_on(source).len(), 2);
    }

    #[test]
    fn flags_ordinary_overloads_without_tagged_template_first_param() {
        // Negative control: an ordinary collapsible group with no
        // `TemplateStringsArray` first param still flags — it collapses cleanly
        // into `f(x: string | number): number`.
        let source = "
function f(x: string): number;
function f(x: number): number;
function f(x: string | number): number { return 0; }
";
        assert_eq!(run_on(source).len(), 2);
    }

    #[test]
    fn flags_full_cartesian_product_multi_position() {
        // Two positions vary and every combination is unique, but they fill the
        // FULL cartesian product ({'x','y'} × {'on','off'} = 4 of 4), so the
        // overloads are exactly `f('x' | 'y', 'on' | 'off')` and still flag.
        let source = "
function f(a: 'x', b: 'on'): void;
function f(a: 'x', b: 'off'): void;
function f(a: 'y', b: 'on'): void;
function f(a: 'y', b: 'off'): void;
function f(a: 'x' | 'y', b: 'on' | 'off'): void {}
";
        assert_eq!(run_on(source).len(), 4);
    }
}
