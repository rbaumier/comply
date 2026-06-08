use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

fn is_test_step_call(call: &oxc_ast::ast::CallExpression<'_>) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else { return false };
    if member.property.name != "step" {
        return false;
    }
    let Expression::Identifier(obj) = &member.object else { return false };
    obj.name == "test"
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["step"])
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

        let AstKind::CallExpression(call) = node.kind() else { return };
        if !is_test_step_call(call) {
            return;
        }

        if !is_inside_step(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "playwright-no-nested-step".into(),
            message: "Do not nest `test.step()` methods.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_inside_step(
    node: &oxc_semantic::AstNode<'_>,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();
    let mut id = node.id();
    loop {
        let parent_id = nodes.parent_id(id);
        if parent_id == id {
            break;
        }
        id = parent_id;
        match nodes.kind(id) {
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                // Check if this function's parent is a test.step() call
                let call_id = nodes.parent_id(id);
                if call_id == id {
                    continue;
                }
                if let AstKind::CallExpression(call) = nodes.kind(call_id)
                    && is_test_step_call(call) {
                        return true;
                    }
            }
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_oxc_ts_with_path;

    const PW_IMPORT: &str = "import { test, expect } from \"@playwright/test\";\n";


    fn run_oxc_ts(source: &str) -> Vec<Diagnostic> {
        run_oxc_ts_with_path(&format!("{PW_IMPORT}{source}"), &Check, "app.test.ts")
    }


    #[test]
    fn flags_nested_step() {
        let src = r#"test.step('outer', async () => {
    await test.step('inner', async () => {});
});"#;
        let d = run_oxc_ts(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-nested-step");
    }


    #[test]
    fn allows_flat_steps() {
        let src = r#"test.step('a', async () => {});
test.step('b', async () => {});"#;
        let d = run_oxc_ts(src);
        assert!(d.is_empty());
    }
}
