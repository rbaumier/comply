//! no-os-command backend for Rust.
//!
//! Flags `Command::new(...)` and `std::process::Command` usage — potential
//! command-injection vectors in Rust code.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");

    // Match `Command::new(...)` or `std::process::Command::new(...)`
    if callee_text.ends_with("Command::new") {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-os-command".into(),
            message: "OS command execution via `Command::new` — potential command-injection vector.".into(),
            severity: Severity::Error,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_command_new() {
        assert_eq!(
            run_on(r#"fn f() { Command::new("sh").arg("-c").arg(input); }"#).len(),
            1,
        );
    }

    #[test]
    fn flags_fully_qualified_command() {
        assert_eq!(
            run_on(r#"fn f() { std::process::Command::new("ls"); }"#).len(),
            1,
        );
    }

    #[test]
    fn allows_non_command_calls() {
        assert!(run_on("fn f() { Builder::new(); }").is_empty());
    }
}
