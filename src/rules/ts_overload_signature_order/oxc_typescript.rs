//! ts-overload-signature-order OXC backend — overloads ordered specific-to-general.
//!
//! Two overloads are compared on arity only when the arity sequence between them
//! decreases somewhere. Inside a non-decreasing run (the `flow`/`pipe`/`compose`
//! pipeline idiom, runs of same-arity type-discriminated siblings, and each
//! family of a data-first/data-last pair) TypeScript dispatches by arity — and by
//! declaration order within one arity — so the ascending order is required and
//! never misorders. Same-arity type-specificity checks still apply across the
//! whole group.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::collections::BTreeSet;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            if let AstKind::Program(program) = node.kind() {
                check_statements(&program.body, ctx, &mut diagnostics);
            }
        }

        diagnostics
    }
}

/// Per-parameter type descriptor used when comparing two overloads.
///
/// `score` is a coarse specificity scalar (lower = more specific). `names`,
/// when present, is the set of named types in the parameter's type annotation
/// (a single `TSTypeReference` or a union of them). It is `None` whenever the
/// annotation is absent or not a clean union of named types (keyword, literal,
/// generic, function type, …), in which case the comparison falls back to
/// `shape` and the scalar `score`.
struct ParamType {
    score: u32,
    shape: TypeShape,
    names: Option<BTreeSet<String>>,
}

/// Where an annotation sits in the syntactic generality lattice, for the
/// comparisons the scalar score cannot decide.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TypeShape {
    /// A named reference (`Role`, `Uppercase<Role>`), a conditional, indexed or
    /// mapped type, a function or object shape — or a union containing one.
    /// Where it sits relative to a primitive needs the type checker: `type
    /// RoleChar = 'p' | 'n'` is narrower than `string`, `type Loose = string`
    /// is not, and a function type is neither.
    Opaque,
    /// `any` / `unknown` — the top of the lattice, more general than anything.
    Top,
    /// A shape the score ranks: literal, primitive keyword, or a union of
    /// those.
    Ranked,
}

/// Signature info for comparison.
struct SigInfo {
    name: String,
    required_params: usize,
    params: Vec<ParamType>,
    /// Head type names of the first parameter's annotation (recursing unions,
    /// ignoring generic arguments), or `None` when the first parameter is
    /// missing or its annotation is not a clean named-type / union-of-named.
    /// Two overloads whose first-parameter head sets are both present and
    /// disjoint accept structurally incompatible discriminating arguments and
    /// have no specific-to-general relationship.
    first_param_heads: Option<BTreeSet<String>>,
    span: oxc_span::Span,
    has_body: bool,
    /// Whether the signature ends in a rest parameter (`...items`). A rest tail
    /// makes the overload a variadic catch-all: it accepts any arity at or above
    /// its fixed params, so when it terminates a non-decreasing-arity pipeline its
    /// (low) counted required-param total must not break the non-decreasing order.
    has_rest: bool,
}

fn extract_sig_info(stmt: &Statement) -> Option<SigInfo> {
    let f = match stmt {
        Statement::FunctionDeclaration(f) => f,
        Statement::ExportNamedDeclaration(exp) => match &exp.declaration {
            Some(Declaration::FunctionDeclaration(f)) => f,
            _ => return None,
        },
        _ => return None,
    };
    let name = f.id.as_ref()?.name.to_string();
    Some(SigInfo {
        name,
        required_params: count_required_params(&f.params),
        params: param_types(&f.params),
        first_param_heads: first_param_type_heads(&f.params),
        span: f.span,
        has_body: f.body.is_some(),
        has_rest: f.params.rest.is_some(),
    })
}

/// Head type names of the first parameter's annotation. Unlike
/// [`union_type_names`], this keeps the head identifier of generic references
/// (`IObservableValue<T>` → `IObservableValue`) because the disjointness check
/// only needs to know whether two overloads name the same container type, not
/// how their generic arguments relate. Returns `None` when there is no first
/// parameter or its annotation is not a named type (or union of named types).
fn first_param_type_heads(params: &FormalParameters) -> Option<BTreeSet<String>> {
    let ann = params.items.first()?.type_annotation.as_ref()?;
    let mut heads = BTreeSet::new();
    if collect_type_heads(&ann.type_annotation, &mut heads) && !heads.is_empty() {
        Some(heads)
    } else {
        None
    }
}

/// Push the head identifier of every named type in `ty` into `heads`, recursing
/// through unions and keeping generic references by their head name. Returns
/// `false` on the first member that is not a named reference, since the head set
/// is only a reliable disjointness basis when every member is a named type.
fn collect_type_heads(ty: &TSType, heads: &mut BTreeSet<String>) -> bool {
    match ty {
        TSType::TSTypeReference(type_ref) => match &type_ref.type_name {
            TSTypeName::IdentifierReference(id) => {
                heads.insert(id.name.to_string());
                true
            }
            _ => false,
        },
        TSType::TSUnionType(union) => union.types.iter().all(|t| collect_type_heads(t, heads)),
        _ => false,
    }
}

fn count_required_params(params: &FormalParameters) -> usize {
    params
        .items
        .iter()
        .filter(|p| {
            // Not optional, not a default value (AssignmentPattern), not rest
            !p.optional && !p.pattern.is_assignment_pattern()
        })
        .count()
}

fn param_types(params: &FormalParameters) -> Vec<ParamType> {
    params
        .items
        .iter()
        .map(|p| match p.type_annotation {
            Some(ref ann) => {
                let (score, shape) = classify_type(&ann.type_annotation);
                ParamType { score, shape, names: union_type_names(&ann.type_annotation) }
            }
            None => ParamType { score: 50, shape: TypeShape::Ranked, names: None },
        })
        .collect()
}

/// Collect the set of named types in a parameter annotation that is a single
/// named type or a union of named types. Returns `None` when the annotation
/// contains anything else (keyword, literal, generic with arguments, function
/// type, …), so the caller falls back to the scalar specificity score rather
/// than assuming a (possibly wrong) overlap relationship.
fn union_type_names(ty: &TSType) -> Option<BTreeSet<String>> {
    let mut names = BTreeSet::new();
    if collect_named_types(ty, &mut names) && !names.is_empty() {
        Some(names)
    } else {
        None
    }
}

/// Push every named type in `ty` into `names`, recursing through unions.
/// Returns `false` (poisoning the whole annotation) on the first member that is
/// not a bare named reference, since a single unparseable member makes the
/// type-name set an unreliable basis for the overlap check.
fn collect_named_types(ty: &TSType, names: &mut BTreeSet<String>) -> bool {
    match ty {
        TSType::TSTypeReference(type_ref) => {
            // Generics (`Foo<T>`) carry arguments whose overlap we cannot judge
            // syntactically; treat the annotation as not a clean named union.
            if type_ref.type_arguments.is_some() {
                return false;
            }
            match &type_ref.type_name {
                TSTypeName::IdentifierReference(id) => {
                    names.insert(id.name.to_string());
                    true
                }
                _ => false,
            }
        }
        TSType::TSUnionType(union) => union.types.iter().all(|t| collect_named_types(t, names)),
        _ => false,
    }
}

/// Where an annotation sits in the generality lattice: `score` orders the shapes
/// that can be ordered syntactically (lower = more specific), `shape` says
/// whether that order means anything against the other parameter.
fn classify_type(ty: &TSType) -> (u32, TypeShape) {
    match ty {
        TSType::TSLiteralType(_) | TSType::TSTemplateLiteralType(_) => (0, TypeShape::Ranked),
        TSType::TSStringKeyword(_)
        | TSType::TSNumberKeyword(_)
        | TSType::TSBooleanKeyword(_)
        | TSType::TSBigIntKeyword(_)
        | TSType::TSSymbolKeyword(_)
        | TSType::TSObjectKeyword(_)
        | TSType::TSNullKeyword(_)
        | TSType::TSUndefinedKeyword(_)
        | TSType::TSVoidKeyword(_)
        | TSType::TSNeverKeyword(_) => (10, TypeShape::Ranked),
        TSType::TSAnyKeyword(_) | TSType::TSUnknownKeyword(_) => (1000, TypeShape::Top),
        TSType::TSUnionType(union) => classify_union(union),
        _ => (50, TypeShape::Opaque),
    }
}

/// A union is exactly as general as its most general member — `'a' | 'b'` still
/// only accepts two literals — with the leaf count as a tie-break so `A | B`
/// ranks just above `A`. One opaque member makes the whole union opaque.
fn classify_union(union: &TSUnionType) -> (u32, TypeShape) {
    let mut widest = 0;
    let mut shape = TypeShape::Ranked;
    for member in &union.types {
        let (score, member_shape) = classify_type(member);
        widest = widest.max(score);
        if member_shape == TypeShape::Opaque {
            shape = TypeShape::Opaque;
        }
    }
    (widest + count_union_leaves(union), shape)
}

fn count_union_leaves(union: &TSUnionType) -> u32 {
    let mut total = 0;
    for ty in &union.types {
        if let TSType::TSUnionType(inner) = ty {
            total += count_union_leaves(inner);
        } else {
            total += 1;
        }
    }
    total
}

/// Line of the first arity reset between overloads `a` and `b` — the step where
/// a lower-arity overload follows a higher-arity one, which is what separates
/// them into two runs instead of two steps of one ascending run. `None` when the
/// span never decreases, i.e. when the two overloads are in the same run and the
/// later one is simply the next, more specific step. Indices past the sequence —
/// the excluded variadic tail — clamp to its last overload, whose own low count
/// is not part of the scan.
fn arity_reset_line(sigs: &[&SigInfo], source: &str, a: usize, b: usize) -> Option<usize> {
    let last = sigs.len().checked_sub(1)?;
    let (a, b) = (a.min(last), b.min(last));
    ((a + 1)..=b)
        .find(|&j| sigs[j - 1].required_params > sigs[j].required_params)
        .map(|j| byte_offset_to_line_col(source, sigs[j].span.start as usize).0)
}

fn check_statements(
    stmts: &[Statement],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let sigs: Vec<Option<SigInfo>> = stmts.iter().map(extract_sig_info).collect();

    let mut i = 0;
    while i < sigs.len() {
        let Some(ref first_sig) = sigs[i] else {
            i += 1;
            continue;
        };
        let name = &first_sig.name;

        // Collect consecutive signatures with the same name.
        let mut group: Vec<&SigInfo> = Vec::new();
        let mut j = i;
        while j < sigs.len() {
            let Some(ref sig) = sigs[j] else { break };
            if sig.name != *name { break; }
            if sig.has_body { break; }
            group.push(sig);
            j += 1;
        }

        if group.len() >= 2 {
            // A pipeline/composition idiom (`flow`, `pipe`, `compose`, zod
            // `partial`/`required`), a run of same-arity type-discriminated
            // sibling overloads (valibot `multipleOf`/`guard`/`required`: a
            // number/bigint pair at arity 1, then the same pair at arity 2), or one
            // family of a data-first/data-last pair (remeda's `purry` idiom):
            // within such a run each successive overload takes at least as many
            // required params as the one before. TypeScript dispatches by arity —
            // and by declaration order within one arity — so this order is required
            // and correct; it never intercepts a call meant for a higher-arity
            // overload. Only a pair whose span decreases somewhere is a candidate
            // misorder. Same-arity siblings remain subject to the type-specificity
            // check below.
            //
            // A final variadic catch-all (`pipe(schema, ...items)`) is the
            // most-general tail of such a pipeline. Its `...items` lands in OXC's
            // `params.rest`, so `count_required_params` counts only the fixed
            // leading params — a total below the specific overloads it generalizes.
            // Exclude that rest tail from the decrease scan so it does not break
            // the sequence; the specific overloads alone must not decrease.
            let specific = match group.last() {
                Some(last) if last.has_rest => &group[..group.len() - 1],
                _ => &group[..],
            };
            'outer: for a in 0..group.len() {
                for b in (a + 1)..group.len() {
                    // Disjoint first-parameter types mean the overloads accept
                    // structurally incompatible discriminating arguments;
                    // TypeScript resolves them regardless of order, so neither
                    // arity nor type-generality implies a misordering.
                    if first_params_disjoint(group[a], group[b]) {
                        continue;
                    }
                    // Flag if earlier has strictly fewer required params.
                    if group[a].required_params >= group[b].required_params {
                        continue;
                    }
                    if let Some(reset_line) = arity_reset_line(specific, ctx.source, a, b) {
                        let (line, column) = byte_offset_to_line_col(
                            ctx.source,
                            group[a].span.start as usize,
                        );
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "Overload of `{name}` is less specific ({ca} params) than a later one ({cb} params), which the arity reset at line {reset_line} puts in a separate run; reorder specific-to-general.",
                                ca = group[a].required_params,
                                cb = group[b].required_params,
                            ),
                            severity: Severity::Error,
                            span: None,
                        });
                        continue 'outer;
                    }
                }
                // Same arity — compare type specificity.
                for b in (a + 1)..group.len() {
                    if group[a].required_params != group[b].required_params { continue; }
                    if first_params_disjoint(group[a], group[b]) { continue; }
                    if earlier_param_types_more_general(group[a], group[b]) {
                        let (line, column) = byte_offset_to_line_col(
                            ctx.source,
                            group[a].span.start as usize,
                        );
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "Overload of `{name}` uses more general parameter types than a later one; reorder specific-to-general."
                            ),
                            severity: Severity::Error,
                            span: None,
                        });
                        continue 'outer;
                    }
                }
            }
        }

        i = j.max(i + 1);
    }
}

/// How parameter `a` relates to the corresponding parameter `b` of a later
/// overload, in terms of specific-to-general ordering.
#[derive(PartialEq, Eq)]
enum ParamRel {
    /// `a` is strictly more general than `b` (e.g. `Foo | Bar` vs `Foo`).
    AMoreGeneral,
    /// `a` is strictly more specific than `b` (already correctly ordered).
    AMoreSpecific,
    /// No specific-to-general relationship: equal, or disjoint named unions.
    Incomparable,
    /// The direction cannot be decided syntactically because one side is
    /// opaque. `a` may well be the more specific of the two.
    Unknown,
}

fn earlier_param_types_more_general(a: &SigInfo, b: &SigInfo) -> bool {
    let ta = &a.params;
    let tb = &b.params;
    if ta.len() != tb.len() || ta.is_empty() {
        return false;
    }
    let mut a_more_general = false;
    for (pa, pb) in ta.iter().zip(tb.iter()) {
        match compare_param(pa, pb) {
            // An undecidable position is the one that may hold the narrowing —
            // remeda's `IsNumericLiteral<N> extends true ? N : never` before
            // `number` — so it blocks the conclusion just like a known-narrower
            // parameter does.
            ParamRel::AMoreSpecific | ParamRel::Unknown => return false,
            ParamRel::AMoreGeneral => a_more_general = true,
            ParamRel::Incomparable => {}
        }
    }
    a_more_general
}

fn compare_param(a: &ParamType, b: &ParamType) -> ParamRel {
    // When both parameters are clean unions of named types, the only genuine
    // specific-to-general relationship is subset/superset. Disjoint sets (no
    // shared type name) are unambiguous to TypeScript regardless of order, so
    // they are Incomparable and must not be flagged.
    if let (Some(na), Some(nb)) = (&a.names, &b.names) {
        return compare_named_sets(na, nb);
    }
    // An opaque annotation hides what it resolves to, so only `any`/`unknown` —
    // the top of the lattice — are known to be more general than it. Every other
    // pairing is a guess in both directions, and guessing it reports the
    // `RoleChar` / `string` narrow-then-fallback pair as a misorder.
    if a.shape != TypeShape::Top
        && b.shape != TypeShape::Top
        && (a.shape == TypeShape::Opaque || b.shape == TypeShape::Opaque)
    {
        return ParamRel::Unknown;
    }
    // Fall back to the coarse specificity score when annotations are absent or
    // too complex to reduce to a clean named-type set.
    if a.score > b.score {
        ParamRel::AMoreGeneral
    } else if a.score < b.score {
        ParamRel::AMoreSpecific
    } else {
        ParamRel::Incomparable
    }
}

/// True when both overloads annotate their first parameter with named types
/// (or unions of named types) whose head sets share no name. Such overloads
/// discriminate on incompatible argument types, so no specific-to-general
/// ordering applies and they must not be flagged. Returns `false` whenever
/// either head set is unknown, so the conservative scoring path still runs.
fn first_params_disjoint(a: &SigInfo, b: &SigInfo) -> bool {
    match (&a.first_param_heads, &b.first_param_heads) {
        (Some(ha), Some(hb)) => ha.is_disjoint(hb),
        _ => false,
    }
}

fn compare_named_sets(a: &BTreeSet<String>, b: &BTreeSet<String>) -> ParamRel {
    if a == b || a.is_disjoint(b) {
        ParamRel::Incomparable
    } else if a.is_superset(b) {
        ParamRel::AMoreGeneral
    } else if a.is_subset(b) {
        ParamRel::AMoreSpecific
    } else {
        // Overlapping but neither contains the other (e.g. {A,B} vs {A,C}):
        // no subtype relationship in either direction.
        ParamRel::Incomparable
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn disjoint_unions_do_not_flag() {
        // Issue #1117: type-predicate overloads over disjoint parameter unions.
        // The three unions share no type name, so no specific-to-general
        // ordering exists and TypeScript resolves them unambiguously.
        let src = "\
export function isUnexpected(
  response: DeleteAnalyzeResult204Response | DeleteAnalyzeResultDefaultResponse,
): response is DeleteAnalyzeResultDefaultResponse;
export function isUnexpected(
  response:
    | AnalyzeDocumentFromStream202Response
    | AnalyzeDocumentFromStreamLogicalResponse
    | AnalyzeDocumentFromStreamDefaultResponse,
): response is AnalyzeDocumentFromStreamDefaultResponse;
export function isUnexpected(
  response: GetAnalyzeResultPdf200Response | GetAnalyzeResultPdfDefaultResponse,
): response is GetAnalyzeResultPdfDefaultResponse;
export function isUnexpected(response: unknown): boolean {
  return false;
}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn guard_overlapping_general_before_specific_still_flags() {
        // `Foo | Bar` (general) declared before `Foo` (specific) over an
        // overlapping union → genuine specific-to-general violation, must fire.
        let src = "\
function f(x: Foo | Bar): void;
function f(x: Foo): void;
function f(x: unknown): void {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn overlapping_specific_before_general_is_allowed() {
        // Correct ordering: the narrower `Foo` comes first, then `Foo | Bar`.
        let src = "\
function f(x: Foo): void;
function f(x: Foo | Bar): void;
function f(x: unknown): void {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn disjoint_arity_observable_overloads_do_not_flag() {
        // Issue #1873: MobX `interceptReads` — 2-param overloads over disjoint
        // observable container types followed by a 3-param object+property
        // overload. The first-parameter types are structurally incompatible, so
        // the lower-arity overloads are not "less specific" than the 3-param one.
        let src = "\
export function interceptReads<T>(value: IObservableValue<T>, handler: ReadInterceptor<T>): Lambda
export function interceptReads<T>(
    observableArray: IObservableArray<T>,
    handler: ReadInterceptor<T>
): Lambda
export function interceptReads<K, V>(
    observableMap: ObservableMap<K, V>,
    handler: ReadInterceptor<V>
): Lambda
export function interceptReads<V>(
    observableSet: ObservableSet<V>,
    handler: ReadInterceptor<V>
): Lambda
export function interceptReads(
    object: Object,
    property: string,
    handler: ReadInterceptor<any>
): Lambda
export function interceptReads(thing, property?, handler?): Lambda {
    return () => {};
}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn progressive_flow_ascending_arity_does_not_flag() {
        // Issue #4447: fp-ts `flow` — successive overloads each add one more
        // required parameter (1 → 2 → 3). TypeScript dispatches by arity, so
        // this ascending order is required and correct, not a misordering.
        let src = "\
export function flow<A extends ReadonlyArray<unknown>, B>(ab: (...a: A) => B): (...a: A) => B
export function flow<A extends ReadonlyArray<unknown>, B, C>(ab: (...a: A) => B, bc: (b: B) => C): (...a: A) => C
export function flow<A extends ReadonlyArray<unknown>, B, C, D>(ab: (...a: A) => B, bc: (b: B) => C, cd: (c: C) => D): (...a: A) => D
export function flow(...fns: Function[]): unknown {
    return fns;
}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn progressive_pipe_ascending_arity_does_not_flag() {
        // Issue #4447: valibot-style `pipe` — ascending arity 2 → 3 → 4.
        let src = "\
export function pipe<A, B, C>(a: A, b: B): C;
export function pipe<A, B, C, D>(a: A, b: B, c: C): D;
export function pipe<A, B, C, D, E>(a: A, b: B, c: C, d: D): E;
export function pipe(...items: unknown[]): unknown {
    return items;
}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn progressive_ascending_arity_two_overloads_does_not_flag() {
        // Issue #4447: a two-overload ascending-arity group (1 → 2 required
        // params over the same base type). Arity is the sole discriminator and
        // ascending order is correct, so this must not flag.
        let src = "\
function f(a: Foo): void;
function f(a: Foo, b: Bar): void;
function f(a: Foo, b?: Bar): void {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn progressive_ascending_arity_with_variadic_tail_does_not_flag() {
        // Issue #6160: valibot `pipe` — specific overloads ascend in arity
        // (2 → 3 → 4) and the final overload before the implementation is a
        // variadic catch-all `(schema, ...items)`. Its `...items` is a rest
        // param, so it counts only 1 required param; that low total must not
        // break the progressive sequence, since TypeScript dispatches the group
        // correctly without reordering.
        let src = "\
export function pipe<TSchema, TItem1>(schema: TSchema, item1: TItem1): Out;
export function pipe<TSchema, TItem1, TItem2>(schema: TSchema, item1: TItem1, item2: TItem2): Out;
export function pipe<TSchema, TItem1, TItem2, TItem3>(schema: TSchema, item1: TItem1, item2: TItem2, item3: TItem3): Out;
export function pipe<TSchema, TItems extends readonly unknown[]>(schema: TSchema, ...items: TItems): Out;
export function pipe(...pipe: unknown[]): unknown {
    return pipe;
}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn variadic_tail_does_not_forgive_internal_misorder() {
        // Negative space for #6160: the rest-tail exemption only excludes the
        // final variadic catch-all from the ascending check; a genuine misorder
        // among the specific overloads (1 → 3 → 2, not strictly ascending) must
        // still flag despite the trailing `...rest` overload.
        let src = "\
function f(a: A): void;
function f(a: A, b: B, c: C): void;
function f(a: A, b: B): void;
function f(...rest: unknown[]): void;
function f(...rest: unknown[]): void {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn same_arity_type_discriminated_groups_within_ascending_do_not_flag() {
        // Issue #6161: valibot `multipleOf` — a number/bigint pair at arity 1,
        // then the same pair at arity 2 (required-param sequence 1, 1, 2, 2).
        // Arity is non-decreasing, so TypeScript dispatches the group correctly in
        // declaration order; the same-arity number/bigint siblings are
        // type-discriminated, not a specific-to-general misorder. Must not flag.
        let src = "\
export function multipleOf<TInput extends number, TRequirement extends number>(
  requirement: TRequirement
): MultipleOfAction<TInput, TRequirement, undefined>;
export function multipleOf<TInput extends bigint, TRequirement extends bigint>(
  requirement: TRequirement
): MultipleOfAction<TInput, TRequirement, undefined>;
export function multipleOf<TInput extends number, TRequirement extends number, TMessage>(
  requirement: TRequirement, message: TMessage
): MultipleOfAction<TInput, TRequirement, TMessage>;
export function multipleOf<TInput extends bigint, TRequirement extends bigint, TMessage>(
  requirement: TRequirement, message: TMessage
): MultipleOfAction<TInput, TRequirement, TMessage>;
export function multipleOf(requirement: unknown, message?: unknown): unknown {
  return requirement;
}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn non_monotonic_arity_still_flags() {
        // Negative space for #6161: relaxing the progressive check to
        // non-decreasing must not silence a genuinely non-monotonic group. Arity
        // 1 → 3 → 2 decreases at the end, so it is not a clean pipeline; the
        // 1-param overload preceding the 3-param one is a real specific-to-general
        // arity misorder and must still flag.
        let src = "\
function f(a: A): void;
function f(a: A, b: B, c: C): void;
function f(a: A, b: B): void;
function f(a: unknown, b?: unknown, c?: unknown): void {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn literal_union_before_keyword_fallback_does_not_flag() {
        // Issue #8119: chessops `parseCommentShapeColor` — a union of string
        // literals in front of a `string` fallback. A union is exactly as
        // general as its most general member, so four literals are far narrower
        // than "any string" and the pair is already ordered specific-to-general.
        let src = "\
function f(str: 'G' | 'R' | 'Y' | 'B'): Color;
function f(str: string): Color | undefined;
function f(str: string): Color | undefined {
    return undefined;
}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn named_reference_union_before_keyword_fallback_does_not_flag() {
        // Issue #8119: chessops `charToRole` — `RoleChar | Uppercase<RoleChar>`
        // in front of a `string` fallback. The named members carry type
        // arguments, so the annotation is not a clean named-type set; what they
        // resolve to is unknowable syntactically, and guessing them more general
        // than `string` reports the correct order as the incorrect one.
        let src = "\
export function charToRole(ch: RoleChar | Uppercase<RoleChar>): Role;
export function charToRole(ch: string): Role | undefined;
export function charToRole(ch: string): Role | undefined {
    return undefined;
}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn conditional_type_before_keyword_fallback_does_not_flag() {
        // Issue #8119: remeda `endsWith` — a conditional type narrowing the
        // suffix to a literal (`string extends Suffix ? never : Suffix`) in
        // front of the widening `string` fallback. What the conditional
        // resolves to needs the type checker, exactly like a named reference,
        // so it is not "more general than `string`".
        let src = "\
export function endsWith<T extends string, Suffix extends string>(
  data: T,
  suffix: string extends Suffix ? never : Suffix,
): data is T & `${string}${Suffix}`;
export function endsWith(data: string, suffix: string): boolean;
export function endsWith(data: string, suffix: string): boolean {
  return data.endsWith(suffix);
}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn opaque_parameter_blocks_a_wider_sibling_parameter() {
        // Issue #8119: remeda `hasAtLeast` — the refining overload writes its
        // first parameter as `IterableContainer | T`, a superset of the
        // fallback's `IterableContainer`, and does the narrowing in the second
        // parameter, whose conditional type is opaque. The undecidable position
        // is the one carrying the refinement, so "wider in the first parameter"
        // does not make the overload more general.
        let src = "\
export function hasAtLeast<T extends IterableContainer, N extends number>(
  data: IterableContainer | T,
  minimum: IsNumericLiteral<N> extends true ? N : never,
): data is ArrayRequiredPrefix<T, N>;
export function hasAtLeast(data: IterableContainer, minimum: number): boolean;
export function hasAtLeast(data: IterableContainer, minimum: number): boolean {
  return data.length >= minimum;
}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn named_reference_before_keyword_does_not_flag() {
        // Issue #8119: a bare named reference in front of a keyword. An alias is
        // normally a narrowing of the primitive it is built from, and nothing in
        // the annotation says otherwise, so the two are incomparable.
        let src = "\
function f(x: Alias): A;
function f(x: string): B;
function f(x: string): unknown {
    return x;
}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn keyword_before_named_reference_does_not_flag() {
        // Negative space for #8119: incomparability is symmetric. `write(data:
        // string)` before `write(data: Buffer)` is a disjoint pair, not a
        // misorder — ranking keywords above named references would only move the
        // false positive to the mirror case.
        let src = "\
function write(data: string): void;
function write(data: Buffer): void;
function write(data: unknown): void {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn keyword_before_literal_union_still_flags() {
        // Negative space for #8119: scoring a union from its members must keep
        // the genuine misorder — `string` accepts every argument the literal
        // union does, so it shadows it.
        let src = "\
function f(x: string): A;
function f(x: 'a' | 'b'): B;
function f(x: string): unknown {
    return x;
}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn keyword_union_before_member_keyword_still_flags() {
        // Negative space for #8119: a union of keywords is more general than one
        // of its members, so `string | number` before `string` still flags.
        let src = "\
function f(x: string | number): A;
function f(x: string): B;
function f(x: unknown): unknown {
    return x;
}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn any_before_keyword_still_flags() {
        // Negative space for #8119: `any`/`unknown` are the top of the lattice,
        // more general than every other annotation including named references.
        let src = "\
function f(x: any): A;
function f(x: string): B;
function f(x: any): unknown {
    return x;
}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn unknown_before_named_reference_still_flags() {
        // Negative space for #8119: a named reference is incomparable to the
        // shapes the score ranks, but never to the top of the lattice.
        let src = "\
function f(x: unknown): A;
function f(x: Alias): B;
function f(x: unknown): unknown {
    return x;
}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn data_first_data_last_families_do_not_flag() {
        // Issue #8345: remeda's `purry` idiom declares two ascending families
        // under one name — data-last (1 → 2 → 3 required params) then
        // data-first (2 → 3 → 4). The single arity reset at the family boundary
        // must not strip the ascending exemption from the steps inside either
        // family, whose overloads are each followed only by higher-arity
        // siblings of the same family.
        let src = "\
export function conditional<T, R0>(case0: Case<T, R0>): (data: T) => R0;
export function conditional<T, R0, R1>(case0: Case<T, R0>, case1: Case<T, R1>): (data: T) => R0 | R1;
export function conditional<T, R0, R1, R2>(case0: Case<T, R0>, case1: Case<T, R1>, case2: Case<T, R2>): (data: T) => R0 | R1 | R2;
export function conditional<T, R0>(data: T, case0: Case<T, R0>): R0;
export function conditional<T, R0, R1>(data: T, case0: Case<T, R0>, case1: Case<T, R1>): R0 | R1;
export function conditional<T, R0, R1, R2>(data: T, case0: Case<T, R0>, case1: Case<T, R1>, case2: Case<T, R2>): R0 | R1 | R2;
export function conditional(...args: readonly unknown[]): unknown {
    return args;
}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn three_concatenated_families_do_not_flag() {
        // Issue #8345: three ascending families (arity 1, 2 each) under one
        // name. Every ascending step stays inside its own family, so none of
        // the two arity resets makes any pair a misorder.
        let src = "\
function f(a: Alpha): void;
function f(a: Alpha, b: Opts): void;
function f(a: Beta): void;
function f(a: Beta, b: Opts): void;
function f(a: Gamma): void;
function f(a: Gamma, b: Opts): void;
function f(a: unknown, b?: Opts): void {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn arity_pair_spanning_a_decrease_still_flags() {
        // Negative space for #8345: the exemption is a property of the span
        // between the two compared overloads, not of the group. In `f(a, b)`,
        // `f(a)`, `f(a, b, c)` the (2nd, 3rd) pair ascends with nothing
        // decreasing between them and is exempt, while the (1st, 3rd) pair
        // spans the 2 → 1 reset and is a genuine misorder: exactly one
        // diagnostic, naming the reset the reader has to look at.
        let src = "\
function f(a: A, b: B): void;
function f(a: A): void;
function f(a: A, b: B, c: C): void;
function f(a: A, b?: B, c?: C): void {}";
        let diagnostics = run(src);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, 1);
        assert!(
            diagnostics[0].message.contains("arity reset at line 2"),
            "{}",
            diagnostics[0].message
        );
    }

    #[test]
    fn non_decreasing_group_still_flags_same_arity_type_misorder() {
        // Negative space for #6161: the non-decreasing exemption gates only the
        // arity check, never the type-specificity check. Within an otherwise
        // non-decreasing group (1, 1, 2, 2), a same-arity pair declared
        // general-before-specific (`Foo | Bar` then `Foo`) is a genuine
        // specific-to-general misorder and must still flag.
        let src = "\
function f(a: Foo | Bar): void;
function f(a: Foo): void;
function f(a: Foo, b: X): void;
function f(a: Foo, b: Y): void;
function f(a: unknown, b?: unknown): void {}";
        assert_eq!(run(src).len(), 1);
    }
}
