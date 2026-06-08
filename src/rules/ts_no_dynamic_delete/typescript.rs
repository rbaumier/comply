//! ts-no-dynamic-delete backend — flag `delete obj[expr]` where `expr` is
//! not a literal string/number.
//!
//! Detection: walk `unary_expression` nodes with operator `delete`, check
//! if the argument is a `subscript_expression` with a non-literal index.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["unary_expression"] => |node, source, ctx, diagnostics|
    // Check operator is "delete"
    let Some(op_node) = node.child_by_field_name("operator") else {
        return;
    };
    let op_text = &source[op_node.byte_range()];
    if op_text != b"delete" {
        return;
    }
    let Some(arg) = node.child_by_field_name("argument") else {
        return;
    };
    // Must be a subscript (computed) access: obj[expr]
    if arg.kind() != "subscript_expression" {
        return;
    }
    let Some(index) = arg.child_by_field_name("index") else {
        return;
    };
    // Allow literal string/number keys and negative numeric literals
    let index_kind = index.kind();
    if index_kind == "string" || index_kind == "number" {
        return;
    }
    // Allow negative number: unary_expression with `-` and number operand
    if index_kind == "unary_expression" {
        let idx_text = &source[index.byte_range()];
        if let Ok(s) = std::str::from_utf8(idx_text) {
            let s = s.trim();
            if s.starts_with('-') && s[1..].trim().parse::<f64>().is_ok() {
                return;
            }
        }
    }
    // Allow `delete process.env[key]` — only way to unset an env var in Node.js.
    if let Some(obj_node) = arg.child_by_field_name("object") {
        if source.get(obj_node.byte_range()).map(|b| b == b"process.env").unwrap_or(false) {
            return;
        }
    }
    let pos = index.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-no-dynamic-delete".into(),
        message: "Do not delete dynamically computed property keys — use `Map` or `Set`.".into(),
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_dynamic_delete() {
        let diags = run_on("delete obj[key];");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_dynamic_delete_expression() {
        let diags = run_on("delete obj[a + b];");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_static_string_delete() {
        assert!(run_on(r#"delete obj["foo"];"#).is_empty());
    }

    #[test]
    fn allows_static_number_delete() {
        assert!(run_on("delete obj[42];").is_empty());
    }

    #[test]
    fn allows_dot_property_delete() {
        assert!(run_on("delete obj.foo;").is_empty());
    }

    // Regression #558 — process.env teardown in tests
    #[test]
    fn allows_delete_process_env_dynamic_key() {
        assert!(run_on("delete process.env[key];").is_empty());
    }

    #[test]
    fn allows_delete_process_env_string_literal() {
        assert!(run_on(r#"delete process.env['MY_VAR'];"#).is_empty());
    }

    #[test]
    fn still_flags_non_process_env_dynamic_delete() {
        let diags = run_on("delete obj[key];");
        assert_eq!(diags.len(), 1);
    }
}
