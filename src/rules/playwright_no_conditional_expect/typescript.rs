//! playwright-no-conditional-expect — flag `expect()` inside conditionals.

use crate::diagnostic::{Diagnostic, Severity};

/// Check if a node is an `expect(...)` call expression.
fn is_expect_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = node.child_by_field_name("function") else {
        return false;
    };
    callee.kind() == "identifier" && callee.utf8_text(source).unwrap_or("") == "expect"
}

/// Check if this node is inside a conditional block (if/switch/catch).
fn is_inside_conditional(node: tree_sitter::Node) -> bool {
    let mut cur = node.parent();
    while let Some(p) = cur {
        match p.kind() {
            "if_statement" | "switch_statement" | "catch_clause" => return true,
            // Don't walk past function boundaries — a function inside a
            // conditional is its own scope.
            "function_declaration"
            | "function"
            | "arrow_function"
            | "method_definition"
            | "generator_function_declaration" => return false,
            _ => {}
        }
        cur = p.parent();
    }
    false
}

/// Test-file check based on path.
const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }
    if !crate::rules::playwright::is_playwright_context(ctx) {
        return;
    }

    if !is_expect_call(node, source) {
        return;
    }

    if !is_inside_conditional(node) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "playwright-no-conditional-expect".into(),
        message: "`expect()` inside a conditional may silently skip — assert unconditionally.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::path::Path;

    const PW: &str = "import { test, expect } from \"@playwright/test\";\n";

    fn run(path: &str, source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(&CheckCtx::for_test(Path::new(path), source), &tree)
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
