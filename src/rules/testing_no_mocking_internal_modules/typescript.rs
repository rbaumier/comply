//! testing-no-mocking-internal-modules backend — detect `vi.mock`/`jest.mock`
//! calls whose first argument is a relative path (`./` or `../`).
//!
//! Why: mocking a file inside your own codebase welds the test to the exact
//! shape of that collaborator. Rename or refactor, and the mock silently
//! stays truthy while the real code broke. Mock boundaries (HTTP, DB,
//! third-party SDKs) and inject internal collaborators instead.

use crate::diagnostic::{Diagnostic, Severity};

fn unquote(raw: &str) -> &str {
    raw.trim_start_matches(['\'', '"', '`'])
        .trim_end_matches(['\'', '"', '`'])
}

/// Is `func` a `vi.mock` / `jest.mock` member expression?
fn is_mock_callee(func: tree_sitter::Node, source: &[u8]) -> bool {
    if func.kind() != "member_expression" { return false; }
    let Some(obj) = func.child_by_field_name("object") else { return false; };
    let Some(prop) = func.child_by_field_name("property") else { return false; };
    let obj_txt = obj.utf8_text(source).unwrap_or("");
    let prop_txt = prop.utf8_text(source).unwrap_or("");
    matches!(obj_txt, "vi" | "jest") && prop_txt == "mock"
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    let Some(func) = node.child_by_field_name("function") else { return; };
    if !is_mock_callee(func, source) { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return; };
    let Some(first) = args.named_child(0) else { return; };
    if !matches!(first.kind(), "string" | "template_string") { return; }
    let raw = first.utf8_text(source).unwrap_or("");
    let path = unquote(raw);

    if path.starts_with("./") || path.starts_with("../") {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &first,
            super::META.id,
            format!(
                "Mocking internal module '{path}' couples tests to implementation details — mock boundaries, not internals."
            ),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_vi_mock_relative_same_dir() {
        assert_eq!(run("vi.mock('./internal');").len(), 1);
    }

    #[test]
    fn flags_vi_mock_relative_parent() {
        assert_eq!(run("vi.mock('../utils/helpers');").len(), 1);
    }

    #[test]
    fn flags_jest_mock_relative() {
        assert_eq!(run("jest.mock('./service');").len(), 1);
    }

    #[test]
    fn allows_mocking_external_package() {
        assert!(run("vi.mock('axios');").is_empty());
    }

    #[test]
    fn allows_mocking_scoped_package() {
        assert!(run("jest.mock('@scope/pkg');").is_empty());
    }

    #[test]
    fn ignores_unrelated_call() {
        assert!(run("foo.mock('./internal');").is_empty());
    }
}
