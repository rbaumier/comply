//! OXC backend for playwright-no-conditional-expect — flag `expect()` inside conditionals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_semantic::NodeId;
use std::sync::Arc;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

fn is_inside_conditional(semantic: &oxc_semantic::Semantic, start_id: NodeId) -> bool {
    let nodes = semantic.nodes();
    let mut cur_id = nodes.parent_id(start_id);
    loop {
        if cur_id == start_id || cur_id == nodes.parent_id(cur_id) {
            return false; // hit root
        }
        let n = nodes.get_node(cur_id);
        match n.kind() {
            AstKind::IfStatement(_) | AstKind::SwitchStatement(_) => return true,
            AstKind::CatchClause(_) => return true,
            // Don't walk past function boundaries
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return false,
            _ => {}
        }
        let next = nodes.parent_id(cur_id);
        if next == cur_id {
            return false;
        }
        cur_id = next;
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        if !is_test_file(ctx.path) {
            return;
        }
        if !crate::rules::playwright::is_playwright_context(ctx) {
            return;
        }

        // Check callee is `expect`
        let oxc_ast::ast::Expression::Identifier(ident) = &call.callee else { return };
        if ident.name.as_str() != "expect" {
            return;
        }

        if !is_inside_conditional(semantic, node.id()) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "playwright-no-conditional-expect".into(),
            message: "`expect()` inside a conditional may silently skip — assert unconditionally.".into(),
            severity: Severity::Warning,
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

    const PW: &str = "import { test, expect } from \"@playwright/test\";\n";

    fn run(path: &str, source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
    }

    #[test]
    fn flags_expect_inside_if() {
        let source = format!("{PW}if (condition) {{\n  expect(value).toBe(true);\n}}");
        let d = run("login.test.ts", &source);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-conditional-expect");
    }

    #[test]
    fn flags_expect_inside_catch() {
        let source = format!(
            "{PW}try {{\n  doSomething();\n}} catch(e) {{\n  expect(e.message).toBe('error');\n}}"
        );
        let d = run("error.test.ts", &source);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_expect_at_top_level() {
        let d = run("login.test.ts", &format!("{PW}expect(value).toBe(true);"));
        assert!(d.is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        let source = format!("{PW}if (condition) {{\n  expect(value).toBe(true);\n}}");
        let d = run("helpers.ts", &source);
        assert!(d.is_empty());
    }
}
