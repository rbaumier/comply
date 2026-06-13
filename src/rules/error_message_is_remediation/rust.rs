//! error-message-is-remediation Rust backend.
//!
//! Flags vague error messages in `panic!("...")`, `anyhow!("...")`,
//! `bail!("...")`, and `Err("...")` / `Err(format!("..."))`.
//!
//! Test code is exempt: files under a test directory, and `panic!` inside
//! inline `#[test]` functions or `#[cfg(test)]` modules, are test-failure
//! signals rather than user-facing errors.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::is_in_test_context;

const VERBS: &[&str] = &[
    "is", "are", "was", "were", "be", "been", "has", "have", "had", "do", "does", "did", "will",
    "would", "could", "should", "may", "might", "must", "shall", "can", "need", "check", "verify",
    "ensure", "provide", "specify", "use", "try", "retry", "pass", "set", "add", "remove",
    "update", "create", "delete", "call", "return", "expect", "require", "missing", "failed",
    "cannot", "unable", "exceeded", "denied", "rejected", "not",
];

fn has_verb(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    VERBS
        .iter()
        .any(|v| lower.split_whitespace().any(|w| w == *v))
}

crate::ast_check! { on ["macro_invocation"] => |node, source, ctx, diagnostics|
    let Some(mac) = node.child_by_field_name("macro") else { return };
    let Ok(mac_name) = mac.utf8_text(source) else { return };

    if ctx.file.path_segments.in_test_dir { return; }

    if mac_name != "panic" && mac_name != "bail" && mac_name != "anyhow" {
        return;
    }

    // Panics inside inline `#[test]` functions / `#[cfg(test)]` modules signal
    // a test failure, not a user-facing error — they need not read as
    // remediation.
    if is_in_test_context(node, source) { return; }

    let Ok(full_text) = node.utf8_text(source) else { return };

    // Extract the first string argument.
    let msg = if let Some(start) = full_text.find('"') {
        let rest = &full_text[start + 1..];
        if let Some(end) = rest.find('"') {
            &rest[..end]
        } else {
            return;
        }
    } else {
        return;
    };

    let too_short = msg.len() < 15;
    let no_verb = !has_verb(msg);

    if too_short || no_verb {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "error-message-is-remediation".into(),
            message: "Error message is too vague — describe what went wrong and what to do.".into(),
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
    use crate::rules::file_ctx::{FileCtx, PathSegments};

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    fn run_on_with_file_ctx(source: &str, file: &FileCtx) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.rs", crate::project::default_static_project_ctx(), file)
    }

    #[test]
    fn flags_short_panic() {
        assert_eq!(run_on(r#"fn f() { panic!("oops"); }"#).len(), 1);
    }

    #[test]
    fn allows_descriptive_panic() {
        assert!(run_on(r#"fn f() { panic!("Connection pool is exhausted — try again or check configuration"); }"#).is_empty());
    }

    #[test]
    fn ignores_panic_in_test_file() {
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..Default::default() },
            ..Default::default()
        };
        assert!(run_on_with_file_ctx(r#"fn f() { panic!("oops"); }"#, &file).is_empty());
    }

    #[test]
    fn still_flags_panic_in_production() {
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: false, ..Default::default() },
            ..Default::default()
        };
        assert_eq!(run_on_with_file_ctx(r#"fn f() { panic!("oops"); }"#, &file).len(), 1);
    }

    #[test]
    fn ignores_panic_in_inline_test_fn() {
        let source = r#"#[test]
fn test_make_field_nullable() {
    panic!("Expected Struct type for list items");
}"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn ignores_panic_in_cfg_test_module() {
        let source = r#"#[cfg(test)]
mod tests {
    fn helper() {
        panic!("Expected Struct type for list items");
    }
}"#;
        assert!(run_on(source).is_empty());
    }
}
