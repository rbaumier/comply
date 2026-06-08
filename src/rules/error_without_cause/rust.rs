//! error-without-cause Rust backend.
//!
//! Flags patterns like `anyhow!("{}", e.to_string())` or creating new errors
//! from `.to_string()` without preserving the source via `.context()` or
//! `.source()`. In Rust the idiomatic pattern is `.context("msg")` or
//! wrapping with `#[from]`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["macro_invocation"] => |node, source, ctx, diagnostics|
    let Some(mac) = node.child_by_field_name("macro") else { return };
    let Ok(mac_name) = mac.utf8_text(source) else { return };

    if mac_name != "anyhow" && mac_name != "bail" {
        return;
    }

    let Ok(full_text) = node.utf8_text(source) else { return };

    // Check if the macro arguments reference `.to_string()` or `.message`
    // without also passing `source` or using `.context()`.
    // Also check surrounding context (e.g., `.context()` chained after the macro).
    let parent_text = node
        .parent()
        .and_then(|p| p.utf8_text(source).ok())
        .unwrap_or("");
    let combined = if parent_text.is_empty() { full_text } else { parent_text };

    if (full_text.contains(".to_string()") || full_text.contains(".message"))
        && !combined.contains("source")
        && !combined.contains("context")
        && !combined.contains("cause")
    {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "error-without-cause".into(),
            message: "Error wraps message without preserving cause — use `.context()` or pass `source`.".into(),
            severity: Severity::Error,
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
    fn flags_anyhow_with_to_string() {
        let src = r#"fn f(e: Error) { anyhow!("{}", e.to_string()); }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_anyhow_with_context() {
        let src = r#"fn f(e: Error) { anyhow!("{}", e.to_string()).context("wrapping"); }"#;
        assert!(run_on(src).is_empty());
    }
}
