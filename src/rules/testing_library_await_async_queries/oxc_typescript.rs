//! testing-library-await-async-queries oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// Cypress Testing Library (`@testing-library/cypress`) adds `findBy*` /
/// `findAllBy*` methods to the `cy` object. They return a `Cypress.Chainable`
/// (resolved through Cypress's command queue), not a Promise — awaiting them
/// breaks the test — so a `findBy*` whose receiver chain roots at `cy` is not
/// an unawaited async query.
fn receiver_roots_at_cy(object: &Expression) -> bool {
    match object {
        Expression::Identifier(id) => id.name.as_str() == "cy",
        Expression::StaticMemberExpression(m) => receiver_roots_at_cy(&m.object),
        Expression::CallExpression(call) => match &call.callee {
            Expression::StaticMemberExpression(m) => receiver_roots_at_cy(&m.object),
            Expression::Identifier(id) => id.name.as_str() == "cy",
            _ => false,
        },
        _ => false,
    }
}

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
            Expression::StaticMemberExpression(m) => {
                if receiver_roots_at_cy(&m.object) {
                    return;
                }
                m.property.name.as_str()
            }
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

    // Regression for #1793: `@testing-library/cypress` adds `findBy*` /
    // `findAllBy*` to the `cy` object; they return a Cypress chainable, not a
    // Promise, so unawaited `cy.findBy*` calls are correct and must not flag.
    #[test]
    fn skips_cypress_find_by_queries() {
        let src = r#"
            describe('Select', () => {
              it('should submit and react to changes', () => {
                cy.findByText('buy').click();
                cy.findByText(/t-shirt size/).should('include.text', 'size M');
                cy.findByLabelText(/choose a size/).click();
              });
            });
        "#;
        assert!(run(src, "Select.cy.ts").is_empty(), "{:?}", run(src, "Select.cy.ts"));
    }

    #[test]
    fn skips_cypress_find_by_chained_on_cy() {
        for q in ["cy.get('form').findByRole('button')", "cy.findAllByText('x')"] {
            let src = format!("{q};");
            assert!(run(&src, "t.cy.ts").is_empty(), "{q}: {:?}", run(&src, "t.cy.ts"));
        }
    }
}
