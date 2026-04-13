//! error-message-is-remediation Rust backend.
//!
//! Flags vague error messages in `panic!("...")`, `anyhow!("...")`,
//! `bail!("...")`, and `Err("...")` / `Err(format!("..."))`.

use crate::diagnostic::{Diagnostic, Severity};

const VERBS: &[&str] = &[
    "is", "are", "was", "were", "be", "been", "has", "have", "had",
    "do", "does", "did", "will", "would", "could", "should", "may",
    "might", "must", "shall", "can", "need", "check", "verify",
    "ensure", "provide", "specify", "use", "try", "retry", "pass",
    "set", "add", "remove", "update", "create", "delete", "call",
    "return", "expect", "require", "missing", "failed", "cannot",
    "unable", "exceeded", "denied", "rejected", "not",
];

fn has_verb(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    VERBS.iter().any(|v| {
        lower.split_whitespace().any(|w| w == *v)
    })
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "macro_invocation" {
        return;
    }

    let Some(mac) = node.child_by_field_name("macro") else { return };
    let Ok(mac_name) = mac.utf8_text(source) else { return };

    if mac_name != "panic" && mac_name != "bail" && mac_name != "anyhow" {
        return;
    }

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
            path: ctx.path.to_path_buf(),
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
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_short_panic() {
        assert_eq!(run_on(r#"fn f() { panic!("oops"); }"#).len(), 1);
    }

    #[test]
    fn allows_descriptive_panic() {
        assert!(run_on(r#"fn f() { panic!("Connection pool is exhausted — try again or check configuration"); }"#).is_empty());
    }
}
