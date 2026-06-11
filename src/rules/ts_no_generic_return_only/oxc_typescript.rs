//! OXC backend for ts-no-generic-return-only — flag function generics
//! that are not referenced in any parameter type annotation. A generic
//! parameter referenced inside a function returned by the function (the
//! curried-generic idiom) is not flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, FunctionBody, Statement};
use std::sync::Arc;

/// Absolute byte span `(start, end)` of a function expression returned by
/// `body` — either `return <fn>` in a block body or a concise arrow body that
/// *is* a function. Used to recognise the curried-generic idiom where an outer
/// type parameter is referenced inside the returned (inner) function.
fn returned_function_span(body: &FunctionBody, concise: bool) -> Option<(usize, usize)> {
    let expr = if concise {
        match body.statements.first()? {
            Statement::ExpressionStatement(es) => &es.expression,
            _ => return None,
        }
    } else {
        body.statements.iter().find_map(|s| match s {
            Statement::ReturnStatement(r) => r.argument.as_ref(),
            _ => None,
        })?
    };
    returned_fn_expr_span(expr)
}

fn returned_fn_expr_span(expr: &Expression) -> Option<(usize, usize)> {
    match expr {
        Expression::ArrowFunctionExpression(a) => {
            Some((a.span.start as usize, a.span.end as usize))
        }
        Expression::FunctionExpression(f) => Some((f.span.start as usize, f.span.end as usize)),
        Expression::ParenthesizedExpression(p) => returned_fn_expr_span(&p.expression),
        _ => None,
    }
}

/// Check if a source substring contains an identifier `name` as a word boundary.
/// Simple heuristic: search for the name surrounded by non-alphanumeric chars.
fn source_range_contains_type_param(source: &str, name: &str) -> bool {
    for (i, _) in source.match_indices(name) {
        let before = if i > 0 {
            source.as_bytes()[i - 1]
        } else {
            b' '
        };
        let after_idx = i + name.len();
        let after = if after_idx < source.len() {
            source.as_bytes()[after_idx]
        } else {
            b' '
        };
        let is_boundary = |b: u8| !b.is_ascii_alphanumeric() && b != b'_';
        if is_boundary(before) && is_boundary(after) {
            return true;
        }
    }
    false
}

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
        use oxc_ast::AstKind;
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            let (type_params, params_span, ret_fn_span) = match node.kind() {
                AstKind::Function(func) => {
                    let Some(tp) = &func.type_parameters else { continue };
                    if func.return_type.as_ref().is_some_and(|ann| {
                        matches!(ann.type_annotation, oxc_ast::ast::TSType::TSTypePredicate(_))
                    }) {
                        continue;
                    }
                    let params_span = func.params.span;
                    let ret_fn_span = func
                        .body
                        .as_ref()
                        .and_then(|b| returned_function_span(b, false));
                    (tp, params_span, ret_fn_span)
                }
                AstKind::ArrowFunctionExpression(arrow) => {
                    let Some(tp) = &arrow.type_parameters else { continue };
                    if arrow.return_type.as_ref().is_some_and(|ann| {
                        matches!(ann.type_annotation, oxc_ast::ast::TSType::TSTypePredicate(_))
                    }) {
                        continue;
                    }
                    let params_span = arrow.params.span;
                    let ret_fn_span = returned_function_span(&arrow.body, arrow.expression);
                    (tp, params_span, ret_fn_span)
                }
                _ => continue,
            };

            let params_text =
                &ctx.source[params_span.start as usize..params_span.end as usize];

            for tp in &type_params.params {
                let name = tp.name.name.as_str();
                if source_range_contains_type_param(params_text, name) {
                    continue;
                }
                // Curried-generic idiom: the outer type parameter is referenced
                // inside the function returned by this function (its inner
                // signature/body), e.g. `FilterKey<TSearch>` — a real use, not a
                // missing inference site.
                if ret_fn_span.is_some_and(|(s, e)| {
                    source_range_contains_type_param(&ctx.source[s..e], name)
                }) {
                    continue;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, tp.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Generic parameter `{name}` is not used in any function parameter; \
                         it has no inference site."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }

        diagnostics
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn allows_generic_in_type_guard() {
        let src = "const isSuccess = <T>(x: any): x is { t: 'success'; value: T } => Boolean(x && x.t === 'success');";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_curried_generic_used_in_returned_function_issue_1038() {
        let src = "export function filterKeysOf<TSearch extends ListRouteSearch>() {\n  return <TKeys extends readonly FilterKey<TSearch>[]>(keys: TKeys & FilterKey<TSearch>): TKeys => keys;\n}";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn still_flags_phantom_return_only_generic_issue_1038() {
        // Returns a value (cast), not a function — no inference site.
        assert_eq!(run("function phantom<T>(): T { return null as T; }").len(), 1);
    }

    #[test]
    fn still_flags_truly_unused_generic_issue_1038() {
        assert_eq!(run("function f<U>(x: number): number { return x; }").len(), 1);
    }
}
