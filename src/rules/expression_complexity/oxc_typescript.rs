//! expression-complexity oxc backend — flag lines with 4+ logical/conditional operators.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

/// Count logical/conditional operators on a line: `&&`, `||`, `??`, `?` (ternary).
#[allow(clippy::if_same_then_else)]
fn count_operators(line: &str) -> usize {
    let mut count = 0;
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    let trimmed = line.trim();
    if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
        return 0;
    }

    while i < len {
        if i + 1 < len && bytes[i] == b'&' && bytes[i + 1] == b'&' {
            count += 1;
            i += 2;
        } else if i + 1 < len && bytes[i] == b'|' && bytes[i + 1] == b'|' {
            count += 1;
            i += 2;
        } else if i + 1 < len && bytes[i] == b'?' && bytes[i + 1] == b'?' {
            count += 1;
            i += 2;
        } else if bytes[i] == b'?' {
            if i + 1 < len && bytes[i + 1] == b'.' {
                i += 2;
            } else if i + 1 < len && bytes[i + 1] == b':' {
                // TypeScript optional property marker (e.g. `key?: T`), not a ternary.
                i += 2;
            } else if i + 1 < len && matches!(bytes[i + 1], b']' | b',' | b')' | b'>') {
                // TypeScript optional tuple/type element marker (e.g. `'c'?`, `T?>`),
                // not a ternary — a real ternary's `?` is followed by a consequent.
                i += 2;
            } else {
                count += 1;
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    count
}

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let max_ops = ctx.config.threshold(super::META.id, "max_ops", ctx.lang);
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if count_operators(line) >= max_ops {
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Expression has {max_ops}+ logical/conditional operators — \
                         extract to named variables."
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_line_with_four_operators() {
        let src = "const x = a && b || c ?? d ? e : f;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_three_operators() {
        let src = "const x = a && b || c ? d : e;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_optional_chaining() {
        let src = "const x = a?.b && c?.d || e;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_optional_property_markers_in_type_literal() {
        // Phantom-key marker type — each `?: never` is the constraint, not a ternary.
        let src = "type ReservedFilterKeys = { page?: never; pageSize?: never; q?: never; sort?: never };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_optional_function_parameter_markers() {
        // `?:` in a function signature marks optional params, not ternaries.
        let src = "function f(a?: T, b?: T, c?: T, d?: T): void {}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_conditional_type_with_three_extra_operators() {
        // Conditional type `?` counts as ternary; adding `&&`, `||`, `??` gives 4 total.
        let src = "type T<X> = X extends string ? A && B || C ?? D : E;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn ignores_optional_tuple_element_markers() {
        // `'c'?` markers are optional tuple elements, not ternaries (issue #3318).
        let src = "expectType<readonly [undefined, 'c'?]>(getArrayTail(['a', undefined, 'c'] as readonly ['a', undefined, 'c'?]));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_optional_tuple_and_generic_type_markers() {
        // `(Set<string>)?`, `Set<string>?`, `number?`, `boolean?` are optional markers.
        let src = "expectType<[Set<string>, (Set<string>)?, Set<string>?]>({} as Schema<[string, number?, boolean?], Set<string>>);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_runtime_expression_with_four_real_operators() {
        // Genuine high-complexity runtime ternary/logical chain — must still fire.
        let src = "const x = a ? b : c || d && e ? f : g;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_real_operators_mixed_with_tuple_optional() {
        // One `T?` tuple marker is exempt, but the real operators alone still cross 4.
        let src = "const x = (y as [number?]) ? a && b || c ?? d : e;";
        assert_eq!(run_on(src).len(), 1);
    }
}
