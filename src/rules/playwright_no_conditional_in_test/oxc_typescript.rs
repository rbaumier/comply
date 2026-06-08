//! playwright-no-conditional-in-test OXC backend — flag if/switch/ternary
//! inside test bodies in Playwright test files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const TEST_FNS: &[&str] = &["test", "it"];

/// Walk up from `node` to check if it's inside a test callback.
fn is_inside_test_callback(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let mut cur_id = node.id();
    let mut found_function = false;
    loop {
        let parent = semantic.nodes().parent_node(cur_id);
        match parent.kind() {
            AstKind::Program(_) => return false,
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                found_function = true;
            }
            AstKind::CallExpression(call) if found_function => {
                let name = match &call.callee {
                    oxc_ast::ast::Expression::Identifier(id) => Some(id.name.as_str()),
                    oxc_ast::ast::Expression::StaticMemberExpression(member) => {
                        match &member.object {
                            oxc_ast::ast::Expression::Identifier(id) => Some(id.name.as_str()),
                            _ => None,
                        }
                    }
                    _ => None,
                };
                if let Some(n) = name
                    && TEST_FNS.contains(&n) {
                        return true;
                    }
                found_function = false;
            }
            _ => {}
        }
        cur_id = parent.id();
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::IfStatement,
            AstType::SwitchStatement,
            AstType::ConditionalExpression,
        ]
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
        if !is_inside_test_callback(node, semantic) {
            return;
        }

        let span_start = match node.kind() {
            AstKind::IfStatement(s) => s.span.start,
            AstKind::SwitchStatement(s) => s.span.start,
            AstKind::ConditionalExpression(e) => e.span.start,
            _ => return,
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Avoid having conditionals in tests.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_oxc_ts_with_path;

    const PW_IMPORT: &str = "import { test, expect } from \"@playwright/test\";\n";


    fn run_oxc_ts(source: &str) -> Vec<Diagnostic> {
        run_oxc_ts_with_path(&format!("{PW_IMPORT}{source}"), &Check, "login.test.ts")
    }


    #[test]
    fn flags_if_in_test() {
        let d = run_oxc_ts("test('cond', () => { if (x) { expect(1).toBe(1); } });");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-conditional-in-test");
    }


    #[test]
    fn allows_if_outside_test() {
        let d = run_oxc_ts("if (process.env.CI) { console.log('ci'); }");
        assert!(d.is_empty());
    }


    #[test]
    fn flags_ternary_in_test() {
        let d = run_oxc_ts("test('tern', () => { const v = x ? 1 : 2; });");
        assert_eq!(d.len(), 1);
    }
}
