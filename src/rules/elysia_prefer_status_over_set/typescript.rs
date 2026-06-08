//! elysia-prefer-status-over-set backend — flag `set.status = N` assignments.

use crate::diagnostic::{Diagnostic, Severity};

/// Returns true if `node` is nested inside an `.onError()` callback.
/// In `.onError()`, setting `set.status` separately from the return value
/// is the idiomatic Elysia pattern — the `status()` helper is not usable there.
fn inside_on_error_handler(mut node: tree_sitter::Node, source: &[u8]) -> bool {
    while let Some(parent) = node.parent() {
        if parent.kind() == "call_expression" {
            if let Some(func) = parent.child_by_field_name("function") {
                if func.kind() == "member_expression" {
                    if let Some(prop) = func.child_by_field_name("property") {
                        if prop.utf8_text(source).unwrap_or("") == "onError" {
                            return true;
                        }
                    }
                }
            }
        }
        node = parent;
    }
    false
}

crate::ast_check! { on ["assignment_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(left) = node.child_by_field_name("left") else { return };
    if left.kind() != "member_expression" {
        return;
    }

    let Some(object) = left.child_by_field_name("object") else { return };
    let Some(property) = left.child_by_field_name("property") else { return };
    if object.utf8_text(source).unwrap_or("") != "set" {
        return;
    }
    if property.utf8_text(source).unwrap_or("") != "status" {
        return;
    }

    if inside_on_error_handler(node, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-prefer-status-over-set".into(),
        message: "`set.status = code` is untyped — use `status(code, body)` for type-safe responses.".into(),
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.ts", &crate::project::ProjectCtx::for_test_with_framework("elysia"), crate::rules::file_ctx::default_static_file_ctx())
    }

    #[test]
    fn flags_set_status_assignment() {
        let src = "import { Elysia } from 'elysia';\napp.get('/', ({ set }) => { set.status = 401; return 'no'; });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_numeric_status() {
        let src = "import { Elysia } from 'elysia';\nfunction h(set) { set.status = 500; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_status_helper() {
        let src =
            "import { Elysia, status } from 'elysia';\napp.get('/', () => status(401, 'no'));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "function h(set) { set.status = 401; }";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }

    #[test]
    fn no_fp_in_on_error_handler() {
        // Issue #534: set.status in .onError() is idiomatic — status() helper
        // is not usable when also mutating headers and returning a computed body.
        let src = r#"import { Elysia } from 'elysia';
app.onError({ as: 'global' }, ({ error, code, set }) => {
  set.status = apiError.status;
  return errorToProblem(apiError, requestId);
});"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_set_status_outside_on_error() {
        let src = r#"import { Elysia } from 'elysia';
app.get('/', ({ set }) => { set.status = 404; return 'not found'; });"#;
        assert_eq!(run_on(src).len(), 1);
    }
}
