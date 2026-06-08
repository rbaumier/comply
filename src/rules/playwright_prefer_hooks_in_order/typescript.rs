//! playwright-prefer-hooks-in-order — enforce consistent hook ordering.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const HOOK_ORDER: &[&str] = &["beforeAll", "beforeEach", "afterEach", "afterAll"];

fn hook_index(name: &str) -> Option<usize> {
    HOOK_ORDER.iter().position(|&h| h == name)
}

fn get_hook_name<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    if node.kind() != "call_expression" {
        return None;
    }
    let callee = node.child_by_field_name("function")?;
    let name = match callee.kind() {
        "identifier" => callee.utf8_text(source).ok()?,
        "member_expression" => callee
            .child_by_field_name("property")?
            .utf8_text(source)
            .ok()?,
        _ => return None,
    };
    if HOOK_ORDER.contains(&name) {
        Some(name)
    } else {
        None
    }
}

/// Check hook order within a block.
fn check_hook_order(
    node: tree_sitter::Node,
    source: &[u8],
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut prev_index: Option<usize> = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        // Recurse into describe callbacks.
        if child.kind() == "expression_statement"
            && let Some(expr) = child.named_child(0)
            && expr.kind() == "call_expression"
        {
            if let Some(callee) = expr.child_by_field_name("function") {
                let is_describe = match callee.kind() {
                    "identifier" => callee.utf8_text(source).unwrap_or("") == "describe",
                    "member_expression" => {
                        callee
                            .child_by_field_name("object")
                            .and_then(|o| o.utf8_text(source).ok())
                            .unwrap_or("")
                            == "describe"
                    }
                    _ => false,
                };
                if is_describe {
                    if let Some(args) = expr.child_by_field_name("arguments") {
                        let ac = args.named_child_count();
                        if ac > 0
                            && let Some(cb) = args.named_child(ac - 1)
                            && let Some(body) = cb.child_by_field_name("body")
                        {
                            check_hook_order(body, source, ctx, diagnostics);
                        }
                    }
                    continue;
                }
            }

            if let Some(name) = get_hook_name(expr, source) {
                let idx = hook_index(name).unwrap();
                if let Some(prev) = prev_index
                    && idx < prev
                {
                    let pos = expr.start_position();
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "playwright-prefer-hooks-in-order".into(),
                        message: format!(
                            "`{name}` hooks should be before any `{}` hooks.",
                            HOOK_ORDER[prev]
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                prev_index = Some(idx);
            } else {
                // Non-hook call resets tracking.
                prev_index = None;
            }
        }
    }
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }
    if !crate::rules::playwright::is_playwright_context(ctx) {
        return;
    }
    check_hook_order(node, source, ctx, diagnostics);
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
        crate::rules::test_helpers::run_rule(&Check, &format!("{PW_IMPORT}{source}"), "app.test.ts")
    }

    #[test]
    fn flags_wrong_order() {
        let src = "\
afterEach(() => {});
beforeEach(() => {});";
        let d = run_ts(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("beforeEach"));
    }

    #[test]
    fn allows_correct_order() {
        let src = "\
beforeAll(() => {});
beforeEach(() => {});
afterEach(() => {});
afterAll(() => {});";
        let d = run_ts(src);
        assert!(d.is_empty());
    }
}
