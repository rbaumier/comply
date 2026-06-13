//! testing-library-await-async-queries oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// `react-test-renderer`'s synchronous tree-search methods. They share the
/// `findBy`/`findAllBy` prefix with Testing Library by coincidence but return a
/// `ReactTestInstance` (or array) immediately — they are not Promises and must
/// not be awaited.
const REACT_TEST_RENDERER_QUERIES: &[&str] =
    &["findByType", "findByProps", "findAllByType", "findAllByProps"];

fn is_find_query(name: &str) -> bool {
    if REACT_TEST_RENDERER_QUERIES.contains(&name) {
        return false;
    }
    name.starts_with("findBy") || name.starts_with("findAllBy")
}

/// True if the closest ancestor wraps the call in an await / yield /
/// chains `.then`/`.catch`/`.finally`, or is part of a `return` / `Promise.all([...])`.
fn call_is_awaited<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node.id();
    for _ in 0..6 {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        match parent.kind() {
            AstKind::AwaitExpression(_) | AstKind::YieldExpression(_) => return true,
            AstKind::ReturnStatement(_) => return true,
            AstKind::StaticMemberExpression(member) => {
                if matches!(member.property.name.as_str(), "then" | "catch" | "finally") {
                    return true;
                }
                current_id = parent_id;
            }
            AstKind::CallExpression(call) => {
                if let Expression::StaticMemberExpression(m) = &call.callee
                    && matches!(m.property.name.as_str(), "then" | "catch" | "finally")
                {
                    return true;
                }
                current_id = parent_id;
            }
            _ => return false,
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["findBy", "findAllBy"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let name = match &call.callee {
            Expression::Identifier(id) => id.name.as_str(),
            Expression::StaticMemberExpression(m) => m.property.name.as_str(),
            _ => return,
        };
        if !is_find_query(name) {
            return;
        }
        if call_is_awaited(node, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{name}` returns a Promise — `await` it (or `.then()`). Without \
                 await, the variable holds an unresolved Promise."
            ),
            severity: Severity::Error,
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

    fn run(src: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, path)
    }

    #[test]
    fn flags_unawaited_find_by_query() {
        let src = r#"const el = screen.findByText("hi");"#;
        assert_eq!(run(src, "t.ts").len(), 1);
    }

    #[test]
    fn skips_awaited_find_by_query() {
        let src = r#"const el = await screen.findByText("hi");"#;
        assert!(run(src, "t.ts").is_empty());
    }

    // Regression for #1897: react-test-renderer's `findByType`/`findByProps`
    // (and their `findAllBy*` variants) are synchronous — they return a
    // `ReactTestInstance`, not a Promise — so an unawaited call is correct.
    #[test]
    fn skips_react_test_renderer_find_by_type() {
        let src = r#"const view = tree.root.findByType(View);"#;
        assert!(run(src, "createTheme.test.tsx").is_empty(), "{:?}", run(src, "createTheme.test.tsx"));
    }

    #[test]
    fn skips_react_test_renderer_sync_queries() {
        for name in ["findByProps", "findAllByType", "findAllByProps"] {
            let src = format!("const r = tree.root.{name}(View);");
            assert!(run(&src, "t.tsx").is_empty(), "{name}: {:?}", run(&src, "t.tsx"));
        }
    }
}
