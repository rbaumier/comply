//! ts-overload-signature-order OXC backend — overloads ordered specific-to-general.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
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

/// Signature info for comparison.
struct SigInfo {
    name: String,
    required_params: usize,
    param_specificities: Vec<u32>,
    span: oxc_span::Span,
    has_body: bool,
}

fn extract_sig_info(stmt: &Statement, source: &str) -> Option<SigInfo> {
    match stmt {
        Statement::FunctionDeclaration(f) => {
            let name = f.id.as_ref()?.name.to_string();
            let required = count_required_params(&f.params);
            let specificities = param_specificities(&f.params, source);
            let has_body = f.body.is_some();
            Some(SigInfo {
                name,
                required_params: required,
                param_specificities: specificities,
                span: f.span,
                has_body,
            })
        }
        Statement::ExportNamedDeclaration(exp) => {
            if let Some(Declaration::FunctionDeclaration(f)) = &exp.declaration {
                let name = f.id.as_ref()?.name.to_string();
                let required = count_required_params(&f.params);
                let specificities = param_specificities(&f.params, source);
                let has_body = f.body.is_some();
                Some(SigInfo {
                    name,
                    required_params: required,
                    param_specificities: specificities,
                    span: f.span,
                    has_body,
                })
            } else {
                None
            }
        }
        _ => None,
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

fn param_specificities(params: &FormalParameters, source: &str) -> Vec<u32> {
    params
        .items
        .iter()
        .map(|p| {
            if let Some(ref ann) = p.type_annotation {
                type_specificity_score(&ann.type_annotation, source)
            } else {
                50
            }
        })
        .collect()
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

fn earlier_param_types_more_general(a: &SigInfo, b: &SigInfo) -> bool {
    let ta = &a.param_specificities;
    let tb = &b.param_specificities;
    if ta.len() != tb.len() || ta.is_empty() {
        return false;
    }
    let mut a_more_general = false;
    for (sa, sb) in ta.iter().zip(tb.iter()) {
        if sa < sb { return false; }
        if sa > sb { a_more_general = true; }
    }
    a_more_general
}
