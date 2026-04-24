use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const VALID_SUFFIXES: &[&str] = &["pk", "fk", "key", "chk", "exl", "idx"];

fn extract_constraint_name(line: &str) -> Option<String> {
    let upper = line.to_ascii_uppercase();
    let idx = upper.find("CONSTRAINT ")?;
    let after = &line[idx + "CONSTRAINT ".len()..].trim_start();
    let mut name = String::new();
    for ch in after.chars() {
        if ch.is_alphanumeric() || ch == '_' || ch == '"' {
            name.push(ch);
        } else {
            break;
        }
    }
    let cleaned = name.replace('"', "");
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

fn has_valid_suffix(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    VALID_SUFFIXES
        .iter()
        .any(|s| lower.ends_with(&format!("_{s}")))
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if !line.to_ascii_uppercase().contains("CONSTRAINT ") {
                continue;
            }
            if let Some(name) = extract_constraint_name(line)
                && !has_valid_suffix(&name) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Constraint `{name}` must end with _pk|_fk|_key|_chk|_exl|_idx (format: {{table}}_{{col}}_{{suffix}})."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
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
    fn flags_missing_suffix() {
        assert_eq!(
            run("`CONSTRAINT user_email UNIQUE (email)`").len(),
            1
        );
    }

    #[test]
    fn allows_key_suffix() {
        assert!(run("`CONSTRAINT user_email_key UNIQUE (email)`").is_empty());
    }

    #[test]
    fn allows_fk_suffix() {
        assert!(run("`CONSTRAINT order_user_id_fk FOREIGN KEY (user_id) REFERENCES users(id)`").is_empty());
    }
}
