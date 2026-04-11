use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const VERBS: &[&str] = &[
    "is", "are", "was", "were", "be", "been", "has", "have", "had",
    "do", "does", "did", "will", "would", "could", "should", "may",
    "might", "must", "shall", "can", "need", "check", "verify",
    "ensure", "provide", "specify", "use", "try", "retry", "pass",
    "set", "add", "remove", "update", "create", "delete", "call",
    "return", "expect", "require", "missing", "failed", "cannot",
    "unable", "exceeded", "denied", "rejected", "not",
];

/// Extract the string literal from `new Error("...")` or `new Error('...')`.
fn extract_error_message(line: &str) -> Option<&str> {
    let pos = line.find("new Error(")?;
    let after = &line[pos + 10..];
    let quote = after.chars().next()?;
    if quote != '"' && quote != '\'' && quote != '`' {
        return None;
    }
    let inner = &after[1..];
    let end = inner.find(quote)?;
    Some(&inner[..end])
}

fn has_verb(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    VERBS.iter().any(|v| {
        lower.split_whitespace().any(|w| w == *v)
    })
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(msg) = extract_error_message(line) {
                let too_short = msg.len() < 15;
                let no_verb = !has_verb(msg);
                if too_short || no_verb {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "error-message-is-remediation".into(),
                        message: format!(
                            "Error message \"{msg}\" is too vague — describe what went wrong and what to do about it."
                        ),
                        severity: Severity::Warning,
                    });
                }
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_short_message() {
        assert_eq!(run(r#"throw new Error("Invalid");"#).len(), 1);
    }

    #[test]
    fn flags_noun_only_message() {
        assert_eq!(run(r#"throw new Error("Configuration");"#).len(), 1);
    }

    #[test]
    fn allows_descriptive_message() {
        assert!(run(r#"throw new Error("User not found — verify the ID and retry");"#).is_empty());
    }

    #[test]
    fn allows_message_with_verb() {
        assert!(run(r#"throw new Error("Cannot connect to the database server");"#).is_empty());
    }
}
