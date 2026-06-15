use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
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

/// True when `expr` is, or syntactically contains, a `CallExpression`.
///
/// Two separate invocations of a call are not referentially guaranteed to be
/// equal — calling twice can return different results — so `expect(f()).toBe(f())`
/// is a meaningful identity / memoization / caching assertion, not a value that is
/// trivially equal to itself. Operands made only of literals, identifiers and
/// call-free member access are deterministic and side-effect-free, so an
/// identical-text self-compare on those is always true and stays flagged.
fn contains_call(expr: &Expression) -> bool {
    use Expression as E;
    match expr {
        E::CallExpression(_) => true,
        E::ParenthesizedExpression(p) => contains_call(&p.expression),
        E::ChainExpression(c) => match &c.expression {
            oxc_ast::ast::ChainElement::CallExpression(_) => true,
            oxc_ast::ast::ChainElement::TSNonNullExpression(n) => contains_call(&n.expression),
            oxc_ast::ast::ChainElement::StaticMemberExpression(m) => contains_call(&m.object),
            oxc_ast::ast::ChainElement::ComputedMemberExpression(m) => {
                contains_call(&m.object) || contains_call(&m.expression)
            }
            oxc_ast::ast::ChainElement::PrivateFieldExpression(m) => contains_call(&m.object),
        },
        E::TSNonNullExpression(n) => contains_call(&n.expression),
        E::TSAsExpression(a) => contains_call(&a.expression),
        E::TSSatisfiesExpression(a) => contains_call(&a.expression),
        E::TSTypeAssertion(a) => contains_call(&a.expression),
        E::TSInstantiationExpression(a) => contains_call(&a.expression),
        E::StaticMemberExpression(m) => contains_call(&m.object),
        E::ComputedMemberExpression(m) => {
            contains_call(&m.object) || contains_call(&m.expression)
        }
        E::PrivateFieldExpression(m) => contains_call(&m.object),
        E::UnaryExpression(u) => contains_call(&u.argument),
        E::AwaitExpression(a) => contains_call(&a.argument),
        E::BinaryExpression(b) => contains_call(&b.left) || contains_call(&b.right),
        E::LogicalExpression(l) => contains_call(&l.left) || contains_call(&l.right),
        E::ConditionalExpression(c) => {
            contains_call(&c.test) || contains_call(&c.consequent) || contains_call(&c.alternate)
        }
        _ => false,
    }
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
        // Call-bearing operands (e.g. `probe.entry(p)`, `f()`, `cache.get(k)`) are not
        // referentially stable across two evaluations, so `.toBe`/`.toEqual` on them is a
        // meaningful identity/memoization assertion, not an always-true self-compare.
        let (Some(actual_expr), Some(expected_expr)) = (
            expect_call.arguments[0].as_expression(),
            call.arguments[0].as_expression(),
        ) else {
            return;
        };
        if contains_call(actual_expr) || contains_call(expected_expr) {
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
}
