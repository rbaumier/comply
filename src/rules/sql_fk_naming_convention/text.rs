use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

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

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let upper = line.to_ascii_uppercase();
            if !upper.contains("FOREIGN KEY") {
                continue;
            }
            let Some(name) = extract_constraint_name(line) else {
                // FK without CONSTRAINT clause => no deterministic name
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "FOREIGN KEY without CONSTRAINT clause — name it `{from_table}_{from_col}_{to_table}_{to_col}_fk`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
                continue;
            };
            let lower = name.to_ascii_lowercase();
            // Must end with _fk and contain at least 4 underscore-separated segments
            let segments: Vec<&str> = lower.split('_').collect();
            let ends_fk = lower.ends_with("_fk");
            let shape_ok = ends_fk && segments.len() >= 5;
            if !shape_ok {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "FK `{name}` must follow `{{from_table}}_{{from_col}}_{{to_table}}_{{to_col}}_fk`."
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
    fn flags_missing_constraint_clause() {
        assert_eq!(
            run("`FOREIGN KEY (user_id) REFERENCES users(id)`").len(),
            1
        );
    }

    #[test]
    fn flags_short_name() {
        assert_eq!(
            run("`CONSTRAINT user_fk FOREIGN KEY (user_id) REFERENCES users(id)`").len(),
            1
        );
    }

    #[test]
    fn allows_full_shape() {
        assert!(run("`CONSTRAINT order_user_id_user_id_fk FOREIGN KEY (user_id) REFERENCES user(id)`").is_empty());
    }
}
