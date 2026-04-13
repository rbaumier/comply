//! error-message-is-remediation — flag vague error messages in
//! `new Error("...")`. Messages should describe what went wrong and
//! what to do about it.
//!
//! Walks the AST looking for `new_expression` nodes constructing `Error`,
//! then inspects the first string argument.

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

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "new_expression" {
        return;
    }

    // Check constructor name is "Error".
    let Some(ctor) = node.child_by_field_name("constructor") else { return };
    if ctor.utf8_text(source).unwrap_or("") != "Error" {
        return;
    }

    // Get the arguments.
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let Some(first_arg) = args.named_child(0) else { return };

    // Extract string content.
    let msg = match first_arg.kind() {
        "string" => {
            let raw = first_arg.utf8_text(source).unwrap_or("");
            // Strip quotes.
            if raw.len() >= 2 {
                &raw[1..raw.len() - 1]
            } else {
                return;
            }
        }
        "template_string" => {
            let raw = first_arg.utf8_text(source).unwrap_or("");
            // Strip backticks.
            if raw.len() >= 2 {
                &raw[1..raw.len() - 1]
            } else {
                return;
            }
        }
        _ => return,
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
            message: format!(
                "Error message \"{msg}\" is too vague \
                 — describe what went wrong and what to do about it."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn has_verb(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    VERBS
        .iter()
        .any(|v| lower.split_whitespace().any(|w| w == *v))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_short_message() {
        assert_eq!(run_on(r#"throw new Error("Invalid");"#).len(), 1);
    }

    #[test]
    fn flags_noun_only_message() {
        assert_eq!(run_on(r#"throw new Error("Configuration");"#).len(), 1);
    }

    #[test]
    fn allows_descriptive_message() {
        assert!(
            run_on(r#"throw new Error("User not found — verify the ID and retry");"#).is_empty()
        );
    }

    #[test]
    fn allows_message_with_verb() {
        assert!(
            run_on(r#"throw new Error("Cannot connect to the database server");"#).is_empty()
        );
    }

    #[test]
    fn ignores_non_error_constructor() {
        assert!(run_on(r#"const msg = "Invalid";"#).is_empty());
    }
}
