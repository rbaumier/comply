use oxc_ast::AstKind;
use oxc_ast::ast::Expression;
use oxc_semantic::NodeId;
use oxc_span::GetSpan;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{source_type_for_path, with_semantic};
use crate::rules::backend::CheckCtx;

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

#[derive(Debug)]
pub struct Check;

impl crate::rules::backend::AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, _tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_type = source_type_for_path(ctx.path);
        with_semantic(ctx.source, source_type, |semantic| {
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
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "no-unused-locators".into(),
                    message: format!(
                        "Locator `{name}` is declared but never used. Call an action or \
                         assertion on it, or remove the declaration."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }

            diagnostics
        })
    }
}

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

fn byte_offset_to_line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, c) in source.char_indices() {
        if i >= byte_offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
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
    fn flags_unused_get_by_role() {
        let src = r#"
test('example', async ({ page }) => {
  const input = page.getByRole('textbox');
  await page.click('a');
});
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("input"));
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
    fn allows_locator_with_fill() {
        let src = r#"
test('example', async ({ page }) => {
  const loc = page.locator('.item');
  await loc.fill('text');
});
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_chained_locator() {
        let src = r#"
test('example', async ({ page }) => {
  const nested = page.locator('.parent').locator('.child');
  await nested.click();
});
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_multiple_unused_locators() {
        let src = r#"
test('example', async ({ page }) => {
  const a = page.locator('.a');
  const b = page.getByTestId('b');
  await page.click('body');
});
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 2);
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
