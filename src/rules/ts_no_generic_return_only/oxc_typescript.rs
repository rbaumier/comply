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
                    if func.return_type.as_ref().is_some_and(|ann| {
                        matches!(ann.type_annotation, oxc_ast::ast::TSType::TSTypePredicate(_))
                    }) {
                        continue;
                    }
                    let params_span = func.params.span;
                    (tp, params_span)
                }
                AstKind::ArrowFunctionExpression(arrow) => {
                    let Some(tp) = &arrow.type_parameters else { continue };
                    if arrow.return_type.as_ref().is_some_and(|ann| {
                        matches!(ann.type_annotation, oxc_ast::ast::TSType::TSTypePredicate(_))
                    }) {
                        continue;
                    }
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

    #[test]
    fn allows_generic_in_type_guard() {
        let src = "const isSuccess = <T>(x: any): x is { t: 'success'; value: T } => Boolean(x && x.t === 'success');";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
