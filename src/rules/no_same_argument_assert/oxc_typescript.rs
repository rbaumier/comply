use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::{byte_offset_to_line_col, expression_is_reproducible};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__") || s.contains("_test.")
}

fn span_text(source: &str, span: oxc_span::Span) -> &str {
    &source[span.start as usize..span.end as usize]
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["expect"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_test_file(ctx.path) {
            return;
        }
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        // Shape: expect(actual).toBe(expected) or .toEqual(expected)
        // callee must be a member expression: <object>.<property>
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let prop = member.property.name.as_str();
        if prop != "toBe" && prop != "toEqual" {
            return;
        }
        // Object must be a call expression: expect(actual)
        let Expression::CallExpression(expect_call) = &member.object else {
            return;
        };
        let Expression::Identifier(expect_id) = &expect_call.callee else {
            return;
        };
        if expect_id.name.as_str() != "expect" {
            return;
        }
        // Both must have exactly one argument.
        if expect_call.arguments.len() != 1 || call.arguments.len() != 1 {
            return;
        }
        let actual_text = span_text(ctx.source, expect_call.arguments[0].span()).trim();
        let expected_text = span_text(ctx.source, call.arguments[0].span()).trim();
        if actual_text.is_empty() || actual_text != expected_text {
            return;
        }
        // Operands that are not reproducible (e.g. `probe.entry(p)`, `f()`, `cache.get(k)`)
        // can return a different value on each of the two evaluations, so `.toBe`/`.toEqual`
        // on them is a meaningful identity/memoization assertion, not an always-true
        // self-compare. A call reaching no binding — `'a'.at(0)` — cannot vary, so it
        // stays an always-true self-compare and is reported.
        let (Some(actual_expr), Some(expected_expr)) = (
            expect_call.arguments[0].as_expression(),
            call.arguments[0].as_expression(),
        ) else {
            return;
        };
        if !expression_is_reproducible(actual_expr) || !expression_is_reproducible(expected_expr) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Asserting a value equals itself — this is always true and tests nothing."
                .into(),
            severity: super::META.severity,
            span: None,
        });
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

    fn run_test_file(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "foo.test.ts")
    }

    #[test]
    fn flags_same_arg_tobe() {
        assert_eq!(run_test_file("  expect(x).toBe(x);").len(), 1);
    }

    #[test]
    fn flags_same_arg_to_equal() {
        assert_eq!(
            run_test_file("  expect(result).toEqual(result);").len(),
            1
        );
    }

    #[test]
    fn allows_different_args() {
        assert!(run_test_file("  expect(actual).toBe(expected);").is_empty());
    }

    #[test]
    fn ignores_non_test_files() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "  expect(x).toBe(x);", "t.ts").is_empty());
    }

    #[test]
    fn allows_identical_call_operands() {
        assert!(run_test_file("  expect(probe.entry(aPath)).toBe(probe.entry(aPath));").is_empty());
    }

    #[test]
    fn allows_identical_bare_call_operands() {
        assert!(run_test_file("  expect(f()).toBe(f());").is_empty());
    }

    #[test]
    fn allows_identical_cache_get_operands() {
        assert!(run_test_file("  expect(cache.get(k)).toBe(cache.get(k));").is_empty());
    }

    #[test]
    fn flags_identical_string_literals() {
        assert_eq!(run_test_file("  expect('a').toBe('a');").len(), 1);
    }

    #[test]
    fn flags_identical_member_access() {
        assert_eq!(run_test_file("  expect(obj.prop).toBe(obj.prop);").len(), 1);
    }

    #[test]
    fn flags_identical_object_literals() {
        assert_eq!(run_test_file("  expect({ a: 1 }).toEqual({ a: 1 });").len(), 1);
    }

    #[test]
    fn allows_object_literals_holding_a_call() {
        assert!(run_test_file("  expect({ a: f() }).toEqual({ a: f() });").is_empty());
    }

    // A call whose callee and arguments reach no binding returns the same value
    // twice, so asserting one against the other tests nothing (issue #8241).
    #[test]
    fn flags_identical_literal_rooted_call_operands() {
        assert_eq!(run_test_file("  expect('a'.at(0)).toBe('a'.at(0));").len(), 1);
    }

    // Spreading an iterable drains it, so the two arrays hold different elements.
    #[test]
    fn allows_spread_array_operands() {
        assert!(run_test_file("  expect([...gen]).toEqual([...gen]);").is_empty());
    }

    // Awaiting the same settled promise twice yields the same value, so the
    // assertion is still vacuous; awaiting two calls is not.
    #[test]
    fn flags_identical_awaited_bindings() {
        assert_eq!(run_test_file("  expect(await p).toBe(await p);").len(), 1);
    }

    #[test]
    fn allows_awaited_calls() {
        assert!(run_test_file("  expect(await load()).toBe(await load());").is_empty());
    }
}
