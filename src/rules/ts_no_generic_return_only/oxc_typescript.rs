//! OXC backend for ts-no-generic-return-only — flag function generics
//! that are not referenced in any parameter type annotation.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

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
            let (type_params, params_span) = match node.kind() {
                AstKind::Function(func) => {
                    let Some(tp) = &func.type_parameters else { continue };
                    let params_span = func.params.span;
                    (tp, params_span)
                }
                AstKind::ArrowFunctionExpression(arrow) => {
                    let Some(tp) = &arrow.type_parameters else { continue };
                    let params_span = arrow.params.span;
                    (tp, params_span)
                }
                _ => continue,
            };

            let params_text =
                &ctx.source[params_span.start as usize..params_span.end as usize];

            for tp in &type_params.params {
                let name = tp.name.name.as_str();
                if !source_range_contains_type_param(params_text, name) {
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
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_generic_only_in_return() {
        let src = "function parse<T>(): T { return {} as T; }";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn flags_arrow_generic_only_in_return() {
        let src = "const f = <T>(): T => ({} as T);";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn allows_generic_used_in_parameter() {
        let src = "function identity<T>(x: T): T { return x; }";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_non_generic_function() {
        let src = "function plain(): string { return 'x'; }";
        assert!(run(src).is_empty());
    }
}
