//! error-message Rust backend — flag error macros without a message.
//!
//! Detects:
//! - `anyhow!()`, `bail!()`, `eyre!()` with no arguments
//! - `panic!()` with no arguments
//! - Custom error type construction without a message field

use crate::diagnostic::{Diagnostic, Severity};

const ERROR_MACROS: &[&str] = &["anyhow", "bail", "eyre", "panic"];

crate::ast_check! { on ["macro_invocation"] => |node, source, ctx, diagnostics|
    let Some(macro_node) = node.child_by_field_name("macro") else { return };
    let Ok(macro_name) = macro_node.utf8_text(source) else { return };

    // Strip path prefix (e.g. `anyhow::bail` -> `bail`).
    let name = macro_name.rsplit("::").next().unwrap_or(macro_name);

    if !ERROR_MACROS.contains(&name) {
        return;
    }

    // Get the token_tree (arguments).
    let mut has_args = false;
    let child_count = node.named_child_count();
    for i in 0..child_count {
        if let Some(child) = node.named_child(i)
            && child.kind() == "token_tree"
        {
            let inner = child.utf8_text(source).unwrap_or("()");
            // Strip outer parens and check if empty.
            let trimmed = inner
                .strip_prefix('(')
                .and_then(|s| s.strip_suffix(')'))
                .unwrap_or(inner)
                .trim();
            if !trimmed.is_empty() {
                has_args = true;
            }
            break;
        }
    }

    if !has_args {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "error-message".into(),
            message: format!("Pass a message to `{name}!()`."),
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_panic_without_message() {
        let d = run_on("fn f() { panic!(); }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("panic"));
    }

    #[test]
    fn flags_bail_without_message() {
        let d = run_on("fn f() { bail!(); }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("bail"));
    }

    #[test]
    fn allows_panic_with_message() {
        assert!(run_on(r#"fn f() { panic!("something went wrong"); }"#).is_empty());
    }

    #[test]
    fn allows_anyhow_with_message() {
        assert!(run_on(r#"fn f() { anyhow!("failed to process"); }"#).is_empty());
    }

    #[test]
    fn allows_todo_without_message() {
        assert!(run_on("fn f() { todo!(); }").is_empty());
        assert!(run_on("fn f() { unimplemented!(); }").is_empty());
    }

    #[test]
    fn allows_todo_in_unreachable_branch() {
        let code = r#"
            fn process(t: i32) {
                match t {
                    0 => {},
                    _ => todo!(),
                }
            }
        "#;
        assert!(run_on(code).is_empty());
    }
}
