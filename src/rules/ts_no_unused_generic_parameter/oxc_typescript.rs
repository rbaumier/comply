//! ts-no-unused-generic-parameter OXC backend — flag generic parameters
//! not referenced in function parameters or return type. A generic parameter
//! referenced inside a function returned by the function (the curried-generic
//! idiom) is not flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
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

pub struct Check;

/// Check if `needle` identifier name appears anywhere in the source range.
fn source_contains_ident(source: &str, start: u32, end: u32, needle: &str) -> bool {
    let slice = &source[start as usize..end as usize];
    // Simple word-boundary check: find occurrences of needle that are not
    // part of a longer identifier.
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
            let (type_params, params, return_type, ret_fn_span, body_span) = match node.kind() {
                AstKind::Function(f) => (
                    f.type_parameters.as_deref(),
                    f.params.span,
                    f.return_type.as_ref().map(|r| r.span),
                    f.body.as_ref().and_then(|b| returned_function_span(b, false)),
                    f.body.as_ref().map(|b| b.span),
                ),
                AstKind::ArrowFunctionExpression(f) => (
                    f.type_parameters.as_deref(),
                    f.params.span,
                    f.return_type.as_ref().map(|r| r.span),
                    returned_function_span(&f.body, f.expression),
                    Some(f.body.span),
                ),
                _ => continue,
            };

            let Some(type_params) = type_params else {
                continue;
            };

            for (i, tp) in type_params.params.iter().enumerate() {
                let name = tp.name.name.as_str();

                // Check if used in other type params (constraints/defaults)
                let mut used_in_other_tp = false;
                for (j, other) in type_params.params.iter().enumerate() {
                    if i == j {
                        continue;
                    }
                    if source_contains_ident(
                        ctx.source,
                        other.span.start,
                        other.span.end,
                        name,
                    ) {
                        used_in_other_tp = true;
                        break;
                    }
                }

                let used_in_params =
                    source_contains_ident(ctx.source, params.start, params.end, name);

                let used_in_return = return_type.is_some_and(|r| {
                    source_contains_ident(ctx.source, r.start, r.end, name)
                });

                // Curried-generic idiom: the outer type parameter is referenced
                // inside the function returned by this function (its inner
                // signature/body), e.g. `FilterKey<TSearch>` — a real use.
                let used_in_returned_fn = ret_fn_span.is_some_and(|(s, e)| {
                    source_contains_ident(ctx.source, s as u32, e as u32, name)
                });

                // Used anywhere in the function body span — as a type argument in
                // a call, a type assertion, a callback (useMemo/useCallback), or
                // the parameter types of a returned object literal's methods. The
                // body span excludes the type-parameter list, so the param's own
                // declaration/constraint cannot be miscounted as a use.
                let used_in_body = body_span.is_some_and(|b| {
                    source_contains_ident(ctx.source, b.start, b.end, name)
                });

                if !used_in_params
                    && !used_in_return
                    && !used_in_other_tp
                    && !used_in_returned_fn
                    && !used_in_body
                {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, tp.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Generic parameter `{name}` is not referenced in parameters or return type."
                        ),
                        severity: Severity::Warning,
                        span: Some((
                            tp.span.start as usize,
                            (tp.span.end - tp.span.start) as usize,
                        )),
                    });
                }
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
    fn flags_fully_unused_generic() {
        let diags = run("function f<T>(x: number): string { return ''; }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_generic_in_param() {
        assert!(run("function f<T>(x: T): void {}").is_empty());
    }

    #[test]
    fn allows_generic_in_return() {
        assert!(run("function f<T>(): T { return {} as T; }").is_empty());
    }

    #[test]
    fn allows_generic_constraint_referencing_other() {
        assert!(run("function f<T extends U, U>(x: T): U { return x; }").is_empty());
    }

    #[test]
    fn allows_curried_generic_used_in_returned_function_issue_1038() {
        let src = "export function filterKeysOf<TSearch extends ListRouteSearch>() {\n  return <TKeys extends readonly FilterKey<TSearch>[]>(keys: TKeys & FilterKey<TSearch>): TKeys => keys;\n}";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn still_flags_truly_unused_generic_issue_1038() {
        assert_eq!(run("function f<U>(x: number): number { return x; }").len(), 1);
    }

    #[test]
    fn allows_generic_as_type_argument_in_body_call_issue_1981() {
        let src = "function f<Input>(): unknown {\n  return createPointScale<Exclude<Input, null>>({});\n}";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_generic_in_type_assertion_in_body_issue_1981() {
        let src = "function f<Input>(scale: unknown): unknown {\n  return scale as unknown as ScaleTime<Input>;\n}";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_generic_inside_callback_in_body_issue_1981() {
        let src = "function f<RawDatum>(data: unknown[]): unknown {\n  return useMemo(() => computeForces<RawDatum>({ data }), [data]);\n}";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_generic_in_returned_object_literal_method_param_issue_1981() {
        let src = "function f<Datum>(): unknown {\n  return { render: (label: ComputedLabel<Datum>) => label };\n}";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn still_flags_generic_unused_in_params_return_and_body_issue_1981() {
        let src = "function f<Unused>(x: number): number {\n  return x + 1;\n}";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }
}
