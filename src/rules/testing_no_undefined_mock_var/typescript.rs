//! testing-no-undefined-mock-var backend — flag `vi.fn()` / `jest.fn()`
//! assigned to a variable with no `.mockReturnValue`,
//! `.mockResolvedValue`, or `.mockImplementation` configuration anywhere
//! in the file, and no factory argument passed to `fn()`.
//!
//! Why: a bare `const m = vi.fn()` is a mock that always returns
//! `undefined`. Code under test that reads the result silently gets
//! `undefined`, tests pass for the wrong reason, and a real regression
//! slips through. Either configure the mock or pass an impl to `fn()`.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

crate::ast_check! { on ["variable_declarator"] => |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) { return; }
    // value must be `vi.fn()` or `jest.fn()` with no named child arg.
    let Some(value) = node.child_by_field_name("value") else { return; };
    if value.kind() != "call_expression" { return; }
    let Some(callee) = value.child_by_field_name("function") else { return; };
    if callee.kind() != "member_expression" { return; }
    let Some(obj) = callee.child_by_field_name("object") else { return; };
    let Some(prop) = callee.child_by_field_name("property") else { return; };
    let Ok(obj_text) = obj.utf8_text(source) else { return; };
    let Ok(prop_text) = prop.utf8_text(source) else { return; };
    if prop_text != "fn" { return; }
    if obj_text != "vi" && obj_text != "jest" { return; }

    // If the caller passed an implementation factory, the mock is configured.
    if let Some(args) = value.child_by_field_name("arguments")
        && args.named_child_count() > 0
    {
        return;
    }

    let Some(name_node) = node.child_by_field_name("name") else { return; };
    let Ok(var_name) = name_node.utf8_text(source) else { return; };
    if !var_name.chars().all(|c| c.is_alphanumeric() || c == '_') { return; }

    // Scan the full source for `<var_name>.mockReturnValue|mockResolvedValue|mockImplementation`.
    let configured = ["mockReturnValue", "mockResolvedValue", "mockImplementation"]
        .iter()
        .any(|m| ctx.source_contains(&format!("{var_name}.{m}")));
    if configured { return; }

    // If the mock is used as a spy (appears in expect()), the undefined return is fine.
    if ctx.source_contains(&format!("expect({var_name})")) { return; }

    let pos = value.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "testing-no-undefined-mock-var".into(),
        message: format!(
            "`{var_name}` is a `{obj_text}.fn()` mock with no `.mockReturnValue` / \
             `.mockResolvedValue` / `.mockImplementation` configuration — it will \
             always return `undefined`. Configure it or pass an implementation to \
             `fn(impl)`."
        ),
        severity: Severity::Warning,
        span: None,
    });
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "foo.test.ts")
    }

    fn run_non_test(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "foo.ts")
    }

    #[test]
    fn flags_bare_vi_fn() {
        assert_eq!(run("const m = vi.fn();").len(), 1);
    }

    #[test]
    fn flags_bare_jest_fn() {
        assert_eq!(run("const m = jest.fn();").len(), 1);
    }

    #[test]
    fn allows_configured_mock_return_value() {
        assert!(run("const m = vi.fn(); m.mockReturnValue(1);").is_empty());
    }

    #[test]
    fn allows_configured_mock_resolved_value() {
        assert!(run("const m = jest.fn(); m.mockResolvedValue({ok: true});").is_empty());
    }

    #[test]
    fn allows_configured_mock_implementation() {
        assert!(run("const m = vi.fn(); m.mockImplementation(() => 1);").is_empty());
    }

    #[test]
    fn allows_impl_passed_to_fn() {
        assert!(run("const m = vi.fn(() => 1);").is_empty());
    }

    #[test]
    fn allows_spy_in_expect() {
        assert!(run("const spy = vi.fn(); expect(spy).toHaveBeenCalled();").is_empty());
    }

    #[test]
    fn allows_spy_with_called_with() {
        assert!(run("const handler = jest.fn(); expect(handler).toHaveBeenCalledWith('a', 'b');").is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        assert!(run_non_test("const m = vi.fn();").is_empty());
    }
}
