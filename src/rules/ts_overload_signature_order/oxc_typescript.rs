//! ts-overload-signature-order OXC backend — overloads ordered specific-to-general.

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
/// generic, function type, …), in which case the comparison falls back to the
/// scalar `score`.
struct ParamType {
    score: u32,
    names: Option<BTreeSet<String>>,
}

/// Signature info for comparison.
struct SigInfo {
    name: String,
    required_params: usize,
    params: Vec<ParamType>,
    span: oxc_span::Span,
    has_body: bool,
}

fn extract_sig_info(stmt: &Statement, source: &str) -> Option<SigInfo> {
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
        params: param_types(&f.params, source),
        span: f.span,
        has_body: f.body.is_some(),
    })
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

fn param_types(params: &FormalParameters, source: &str) -> Vec<ParamType> {
    params
        .items
        .iter()
        .map(|p| match p.type_annotation {
            Some(ref ann) => ParamType {
                score: type_specificity_score(&ann.type_annotation, source),
                names: union_type_names(&ann.type_annotation),
            },
            None => ParamType { score: 50, names: None },
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

fn type_specificity_score(ty: &TSType, _source: &str) -> u32 {
    match ty {
        TSType::TSLiteralType(_) | TSType::TSTemplateLiteralType(_) => 0,
        TSType::TSStringKeyword(_)
        | TSType::TSNumberKeyword(_)
        | TSType::TSBooleanKeyword(_)
        | TSType::TSBigIntKeyword(_)
        | TSType::TSSymbolKeyword(_)
        | TSType::TSObjectKeyword(_)
        | TSType::TSNullKeyword(_)
        | TSType::TSUndefinedKeyword(_)
        | TSType::TSVoidKeyword(_)
        | TSType::TSNeverKeyword(_) => 10,
        TSType::TSAnyKeyword(_) | TSType::TSUnknownKeyword(_) => 1000,
        TSType::TSUnionType(union) => 100 + count_union_leaves(union),
        _ => 50,
    }
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

fn check_statements(
    stmts: &[Statement],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let sigs: Vec<Option<SigInfo>> = stmts
        .iter()
        .map(|s| extract_sig_info(s, ctx.source))
        .collect();

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
            'outer: for a in 0..group.len() {
                for b in (a + 1)..group.len() {
                    // Flag if earlier has strictly fewer required params.
                    if group[a].required_params < group[b].required_params {
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
                                "Overload of `{name}` is less specific ({ca} params) than a later one ({cb} params); reorder specific-to-general.",
                                ca = group[a].required_params,
                                cb = group[b].required_params,
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                        continue 'outer;
                    }
                }
                // Same arity — compare type specificity.
                for b in (a + 1)..group.len() {
                    if group[a].required_params != group[b].required_params { continue; }
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
                            severity: Severity::Warning,
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
            ParamRel::AMoreSpecific => return false,
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
}
