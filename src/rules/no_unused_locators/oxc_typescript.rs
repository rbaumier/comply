//! no-unused-locators — OXC backend.
//! Flags Playwright locators declared but never referenced.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::AstKind;
use oxc_ast::ast::Expression;
use oxc_semantic::NodeId;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const LOCATOR_METHODS: &[&str] = &[
    "locator",
    "getByRole",
    "getByText",
    "getByTestId",
    "getByLabel",
    "getByPlaceholder",
    "getByAltText",
    "getByTitle",
    "$",
    "$$",
    "nth",
    "first",
    "last",
    "frameLocator",
];

fn find_var_decl<'a>(
    nodes: &'a oxc_semantic::AstNodes<'a>,
    start: NodeId,
) -> Option<&'a oxc_ast::ast::VariableDeclarator<'a>> {
    let iter = std::iter::once(nodes.kind(start)).chain(nodes.ancestor_kinds(start));
    for kind in iter {
        if let AstKind::VariableDeclarator(decl) = kind {
            return Some(decl);
        }
    }
    None
}

fn is_locator_call(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    match &call.callee {
        Expression::StaticMemberExpression(member) => {
            LOCATOR_METHODS.contains(&member.property.name.as_str())
        }
        _ => false,
    }
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
        let scoping = semantic.scoping();
        let nodes = semantic.nodes();
        let mut diagnostics = Vec::new();

        for symbol_id in scoping.symbol_ids() {
            let decl_id = scoping.symbol_declaration(symbol_id);

            let Some(var_decl) = find_var_decl(nodes, decl_id) else {
                continue;
            };

            let Some(init) = &var_decl.init else { continue };
            if !is_locator_call(init) {
                continue;
            }

            if scoping.get_resolved_references(symbol_id).next().is_some() {
                continue;
            }

            let name = scoping.symbol_name(symbol_id);
            let span = var_decl.id.span();
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Locator `{name}` is declared but never used. Call an action or \
                     assertion on it, or remove the declaration."
                ),
                severity: Severity::Warning,
                span: None,
            });
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
    fn flags_unused_locator() {
        let src = r#"
test('example', async ({ page }) => {
  const button = page.locator('button');
  await page.click('a');
});
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("button"));
    }

    #[test]
    fn allows_locator_with_click() {
        let src = r#"
test('example', async ({ page }) => {
  const button = page.locator('button');
  await button.click();
});
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_locator_in_expect() {
        let src = r#"
test('example', async ({ page }) => {
  const input = page.getByRole('textbox');
  await expect(input).toBeVisible();
});
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_locator_calls() {
        let src = r#"
const user = getUser();
const result = compute(1, 2);
"#;
        assert!(run_on(src).is_empty());
    }
}
