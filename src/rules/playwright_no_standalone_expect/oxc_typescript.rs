//! playwright-no-standalone-expect oxc backend — disallow `expect` outside
//! test blocks.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const TEST_FNS: &[&str] = &["test", "it"];
const HOOK_FNS: &[&str] = &["beforeAll", "beforeEach", "afterAll", "afterEach"];

pub struct Check;

/// Check if a call expression is `expect(...)` or `expect.soft(...)`.
fn is_expect_call(callee: &Expression) -> bool {
    match callee {
        Expression::Identifier(id) => id.name.as_str() == "expect",
        Expression::StaticMemberExpression(member) => {
            if let Expression::Identifier(obj) = &member.object {
                obj.name.as_str() == "expect"
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Returns true if the function node at `node_id` is declared at module (program) level.
/// `ancestor_kinds` starts from the direct parent (does not include the node itself).
fn is_module_level_function(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    for kind in semantic.nodes().ancestor_kinds(node_id) {
        match kind {
            AstKind::Program(_) => return true,
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return false,
            _ => {} // skip intermediate AST wrappers (ExportNamedDeclaration, FunctionBody, etc.)
        }
    }
    false
}

/// Walk up semantic parent nodes to check if this node is inside a test/hook callback.
fn is_inside_test_or_hook<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let mut cur = node.id();
    loop {
        let parent_id = semantic.nodes().parent_id(cur);
        if parent_id == cur {
            break;
        }
        let parent = semantic.nodes().get_node(parent_id);
        // Check if parent is a function/arrow that is an argument to test/it/hook.
        match parent.kind() {
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                // Check if grandparent is a CallExpression argument list.
                let gp_id = semantic.nodes().parent_id(parent_id);
                if gp_id != parent_id {
                    let gp = semantic.nodes().get_node(gp_id);
                    if let AstKind::CallExpression(call) = gp.kind() {
                        let callee_name = match &call.callee {
                            Expression::Identifier(id) => Some(id.name.as_str()),
                            Expression::StaticMemberExpression(member) => {
                                if let Expression::Identifier(obj) = &member.object {
                                    Some(obj.name.as_str())
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        };
                        if let Some(name) = callee_name
                            && (TEST_FNS.contains(&name) || HOOK_FNS.contains(&name)) {
                                return true;
                            }
                    }
                }
                // A module-level function kept for later (declaration, constant,
                // table row) is a helper; one passed to a call runs now.
                if !matches!(
                    semantic.nodes().ancestor_kinds(parent_id).next(),
                    Some(AstKind::CallExpression(_))
                ) && is_module_level_function(parent_id, semantic)
                {
                    return true;
                }
            }
            _ => {}
        }
        cur = parent_id;
    }
    false
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
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_test_file(ctx.path) {
            return;
        }
        if !crate::rules::playwright::is_playwright_context(ctx) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        if !is_expect_call(&call.callee) {
            return;
        }
        if is_inside_test_or_hook(node, semantic) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Expect must be inside of a test block.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    const PW_IMPORT: &str = "import { test, expect } from \"@playwright/test\";\n";

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, &format!("{PW_IMPORT}{source}"), "app.test.ts")
    }

    #[test]
    fn flags_standalone_expect() {
        let d = run_ts("expect(1).toBe(1);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_expect_in_test() {
        let d = run_ts("test('ok', () => { expect(1).toBe(1); });");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_expect_in_helper_called_from_test() {
        let d = run_ts(r#"
function assertUrl(page) {
  expect(page).toHaveURL(/\/dashboard/);
}
test('my test', () => {
  assertUrl(page);
});
"#);
        assert!(d.is_empty(), "expect in helper called from test should be allowed");
    }

    #[test]
    fn allows_expect_in_module_level_arrow_constant() {
        let d = run_ts(r#"
const check = async (page) => {
  expect(page).toHaveURL(/\/dashboard/);
};
test('my test', async () => {
  await check(page);
});
"#);
        assert!(d.is_empty(), "a module-level arrow constant is a helper: {d:?}");
    }

    #[test]
    fn allows_expect_in_module_level_table_row_arrow() {
        let d = run_ts(r#"
const ROWS = [
  { action: "add_contact", check: async (answer) => { expect(answer).toContain("ok"); } },
];
for (const row of ROWS) {
  test(row.action, async () => {
    await row.check("ok");
  });
}
"#);
        assert!(d.is_empty(), "a module-level table row's arrow is a helper: {d:?}");
    }

    #[test]
    fn flags_expect_in_arrow_passed_to_non_test_call() {
        let d = run_ts("[1, 2].forEach((n) => { expect(n).toBe(n); });");
        assert_eq!(d.len(), 1, "a callback passed to a module-level call runs outside any test");
    }
}
