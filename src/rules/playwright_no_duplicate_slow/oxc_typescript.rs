use rustc_hash::FxHashMap;
use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !crate::rules::playwright::is_playwright_context(ctx) {
            return Vec::new();
        }

        // scope_node_id -> list of test.slow() span starts
        let mut by_scope = FxHashMap::<u32, Vec<u32>>::default();

        for node in semantic.nodes().iter() {
            let AstKind::CallExpression(call) = node.kind() else {
                continue;
            };

            // Match `test.slow()` with no arguments
            let Expression::StaticMemberExpression(member) = &call.callee else {
                continue;
            };
            if member.property.name.as_str() != "slow" {
                continue;
            }
            let Expression::Identifier(obj) = &member.object else {
                continue;
            };
            if obj.name.as_str() != "test" {
                continue;
            }
            if !call.arguments.is_empty() {
                continue;
            }

            // Find enclosing function scope (by span start)
            let nodes = semantic.nodes();
            let mut current = node.id();
            let mut scope_key = None;
            loop {
                let pid = nodes.parent_id(current);
                if pid == current {
                    break;
                }
                let parent = nodes.get_node(pid);
                match parent.kind() {
                    AstKind::ArrowFunctionExpression(f) => {
                        scope_key = Some(f.span.start);
                        break;
                    }
                    AstKind::Function(f) => {
                        scope_key = Some(f.span.start);
                        break;
                    }
                    _ => {
                        current = pid;
                    }
                }
            }

            if let Some(key) = scope_key {
                by_scope
                    .entry(key)
                    .or_default()
                    .push(call.span.start);
            }
        }

        let mut diagnostics = Vec::new();
        for spans in by_scope.values() {
            for &span_start in spans.iter().skip(1) {
                let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`test.slow()` is already called in this test; remove the duplicate."
                        .into(),
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
        crate::rules::test_helpers::run_rule(&Check, &format!(
                "import {{ test, expect }} from \"@playwright/test\";
{source}"
            ), "t.ts")
    }

    #[test]
    fn flags_duplicate_slow() {
        let src = r#"
test('my test', () => {
  test.slow();
  test.slow();
});
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_single_slow() {
        let src = r#"
test('my test', () => {
  test.slow();
  expect(1).toBe(1);
});
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_slow_in_different_tests() {
        let src = r#"
test('test1', () => { test.slow(); });
test('test2', () => { test.slow(); });
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_triple_slow() {
        let src = r#"
test('my test', () => {
  test.slow();
  test.slow();
  test.slow();
});
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 2);
    }
}
