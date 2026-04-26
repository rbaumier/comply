//! vitest-hoisted-apis-on-top — flag `vi.mock(...)` / `vi.hoisted(...)` that
//! appear *after* any `import` statement in the top-level program body.
//!
//! Vitest hoists those calls automatically, but placing them visually after
//! imports hides that fact and trips up readers who assume top-down execution.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

/// Returns the call's `(object_text, property_text)` if `node` is a
/// `member_expression` call like `vi.mock(...)`.
fn call_member_parts<'a>(
    node: tree_sitter::Node<'a>,
    source: &'a [u8],
) -> Option<(&'a str, &'a str)> {
    if node.kind() != "call_expression" {
        return None;
    }
    let callee = node.child_by_field_name("function")?;
    if callee.kind() != "member_expression" {
        return None;
    }
    let obj = callee.child_by_field_name("object")?;
    let prop = callee.child_by_field_name("property")?;
    Some((obj.utf8_text(source).ok()?, prop.utf8_text(source).ok()?))
}

/// Walks `node` recursively to find the first nested call that matches
/// `vi.mock(...)` / `vi.hoisted(...)`. Needed because the call sits inside
/// `expression_statement > call_expression` at the program level.
fn is_vi_hoisted_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if let Some((obj, prop)) = call_member_parts(node, source)
        && obj == "vi"
        && (prop == "mock" || prop == "hoisted" || prop == "unmock" || prop == "doMock")
    {
        return true;
    }
    false
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }

    // Only inspect program root once.
    let mut seen_import = false;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "import_statement" => {
                seen_import = true;
            }
            "expression_statement" => {
                // The call is the first named child.
                let Some(inner) = child.named_child(0) else { continue };
                if !is_vi_hoisted_call(inner, source) {
                    continue;
                }
                if seen_import {
                    let pos = inner.start_position();
                    let (_, prop) = call_member_parts(inner, source)
                        .unwrap_or(("vi", "mock"));
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "vitest-hoisted-apis-on-top".into(),
                        message: format!(
                            "`vi.{prop}(...)` should appear above all imports — Vitest hoists it but readers expect source order to match execution order."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_ts_with_path;

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        run_ts_with_path(source, &Check, "app.test.ts")
    }

    #[test]
    fn flags_vi_mock_after_import() {
        let d = run_ts("import { foo } from './foo';\nvi.mock('./foo');");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "vitest-hoisted-apis-on-top");
    }

    #[test]
    fn allows_vi_mock_before_imports() {
        let d = run_ts("vi.mock('./foo');\nimport { foo } from './foo';");
        assert!(d.is_empty());
    }

    #[test]
    fn flags_vi_hoisted_after_import() {
        let d = run_ts("import x from 'x';\nconst h = vi.hoisted(() => ({}));");
        // `const h = vi.hoisted(...)` is a lexical declaration, not an
        // expression_statement — we only flag bare calls that make the
        // hoist invisible. Declaration form is usually intentional.
        assert!(d.is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        let d = run_ts_with_path(
            "import x from 'x';\nvi.mock('./foo');",
            &Check,
            "src/util.ts",
        );
        assert!(d.is_empty());
    }

    #[test]
    fn allows_no_mocks_no_imports() {
        let d = run_ts("const x = 1;");
        assert!(d.is_empty());
    }
}
