//! ts-no-unused-generic-parameter OXC backend — flag generic parameters
//! not referenced in function parameters or return type.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

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
            let (type_params, params, return_type) = match node.kind() {
                AstKind::Function(f) => (
                    f.type_parameters.as_deref(),
                    f.params.span,
                    f.return_type.as_ref().map(|r| r.span),
                ),
                AstKind::ArrowFunctionExpression(f) => (
                    f.type_parameters.as_deref(),
                    f.params.span,
                    f.return_type.as_ref().map(|r| r.span),
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

                if !used_in_params && !used_in_return && !used_in_other_tp {
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
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
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
}
