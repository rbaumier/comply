//! playwright-expect-expect — enforce assertions in test bodies.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

/// Test-defining function names.
const TEST_FNS: &[&str] = &["test", "it"];

/// Returns true if `node` is a `test(…)` or `it(…)` or `test.only(…)` etc.
fn is_test_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = node.child_by_field_name("function") else {
        return false;
    };
    match callee.kind() {
        "identifier" => {
            let name = callee.utf8_text(source).unwrap_or("");
            TEST_FNS.contains(&name)
        }
        "member_expression" => {
            let Some(obj) = callee.child_by_field_name("object") else {
                return false;
            };
            let obj_text = obj.utf8_text(source).unwrap_or("");
            TEST_FNS.contains(&obj_text)
        }
        _ => false,
    }
}

/// Returns true when `node` is (or contains) an `expect(…)` call.
fn contains_expect(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() == "call_expression"
        && let Some(callee) = node.child_by_field_name("function")
    {
        let text = match callee.kind() {
            "identifier" => callee.utf8_text(source).unwrap_or(""),
            "member_expression" => {
                if let Some(obj) = callee.child_by_field_name("object") {
                    obj.utf8_text(source).unwrap_or("")
                } else {
                    ""
                }
            }
            _ => "",
        };
        if text == "expect" {
            return true;
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if contains_expect(child, source) {
            return true;
        }
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }
    if !crate::rules::playwright::is_playwright_context(ctx) {
        return;
    }

    if !is_test_call(node, source) {
        return;
    }

    // The callback is typically the last argument.
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let arg_count = args.named_child_count();
    if arg_count == 0 {
        return;
    }
    let Some(callback) = args.named_child(arg_count - 1) else { return };

    if !contains_expect(callback, source) {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "playwright-expect-expect".into(),
            message: "Test has no assertions.".into(),
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    
    const PW_IMPORT: &str = "import { test, expect } from \"@playwright/test\";\n";

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, &format!("{PW_IMPORT}{source}"), "login.test.ts")
    }

    #[test]
    fn flags_test_without_expect() {
        let d = run_ts("test('should work', () => { const x = 1; });");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-expect-expect");
    }

    #[test]
    fn allows_test_with_expect() {
        let d = run_ts("test('should work', () => { expect(1).toBe(1); });");
        assert!(d.is_empty());
    }

    #[test]
    fn flags_it_without_expect() {
        let d = run_ts("it('works', async () => { await page.click('#btn'); });");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn skips_non_playwright_test_file() {
        let d = crate::rules::test_helpers::run_rule(&Check, "test('should work', () => { const x = 1; });", "login.test.ts");
        assert!(d.is_empty());
    }

    #[test]
    fn skips_non_playwright_file_with_marker_in_string() {
        let d = crate::rules::test_helpers::run_rule(&Check, r#"
const packageName = "@playwright/test";
test('should work', () => { const x = 1; });
"#, "login.test.ts");
        assert!(d.is_empty());
    }
}
