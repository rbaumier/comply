//! expression-complexity oxc backend — flag lines with 4+ logical/conditional operators.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use std::collections::BTreeMap;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let max_ops = ctx.config.threshold(super::META.id, "max_ops", ctx.lang);

        // Count logical (`&&`/`||`/`??`) and conditional (ternary `?`) operators
        // per source line from the AST. Each `LogicalExpression` node is one
        // operator — a chained `a && b && c` nests into two nodes, i.e. two
        // operators — and each `ConditionalExpression` node is one ternary.
        // Counting nodes rather than raw bytes means `?`/`&`/`|` characters
        // inside regex, string, and template literals never count: literal
        // content carries no operator nodes.
        let mut ops_per_line: BTreeMap<usize, usize> = BTreeMap::new();
        for node in semantic.nodes().iter() {
            let start = match node.kind() {
                AstKind::LogicalExpression(expr) => expr.span.start,
                AstKind::ConditionalExpression(expr) => expr.span.start,
                _ => continue,
            };
            let (line, _) = byte_offset_to_line_col(ctx.source, start as usize);
            *ops_per_line.entry(line).or_insert(0) += 1;
        }

        ops_per_line
            .into_iter()
            .filter(|&(_, count)| count >= max_ops)
            .map(|(line, _)| Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "Expression has {max_ops}+ logical/conditional operators — \
                     extract to named variables."
                ),
                severity: Severity::Warning,
                span: None,
            })
            .collect()
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
    fn ignores_type_level_conditional_operators() {
        // A conditional *type* (`X extends string ? ... : ...`) and type-level
        // `&&`/`||`/`??` are not runtime `ConditionalExpression`/`LogicalExpression`
        // nodes, so they carry no operators to count. Counting via the AST (#6439)
        // — not raw bytes — correctly leaves this unflagged.
        let src = "type T<X> = X extends string ? A && B || C ?? D : E;";
        assert!(run_on(src).is_empty());
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

    #[test]
    fn ignores_regex_quantifiers() {
        // Issue #6439: the `?` quantifiers in this regex (`-?`, `)?`, `[+-]?`) are
        // regex syntax, not ternaries — a regex literal carries no operator nodes.
        let src = r#"const JsonSigRx = /^\s*["[{]|^\s*-?\d{1,16}(\.\d{1,17})?([Ee][+-]?\d+)?\s*$/;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_string_literal_operators() {
        // `?`/`:`/`&&`/`||`/`??` inside a string literal are text, not operators.
        let src = r#"const s = "a ? b : c && d || e ?? f";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_template_literal_operators() {
        // Operator characters in a template literal's static text are not operators.
        let src = "const s = `a ? b : c && d || e ?? f`;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_four_real_operators_with_valid_syntax() {
        // `a && b`, `c && d`, `||`, and the ternary `? :` — four operator nodes.
        let src = "const x = a && b || c && d ? e : f;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_three_real_operators_just_below_threshold() {
        // `a && b`, `(a && b) && c`, and the ternary `? :` — three operator nodes.
        let src = "const x = a && b && c ? d : e;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_wrapped_multiline_operator_chain() {
        // A 4-operator chain split across lines is still one over-complex
        // expression: each `&&` is a `LogicalExpression` node attributed to the
        // expression's start line, so the line-wrapped form is flagged like the
        // single-line one.
        let src = "const ok =\n  a &&\n  b &&\n  c &&\n  d &&\n  e;";
        assert_eq!(run_on(src).len(), 1);
    }
}
