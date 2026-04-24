use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let upper = line.to_ascii_uppercase();
            // Require BOOLEAN or BOOL keyword
            let kw_pos = upper.find(" BOOLEAN").or_else(|| upper.find(" BOOL "));
            let Some(pos) = kw_pos else { continue };
            // Extract the column name: last identifier before the keyword on this line
            let prefix = &line[..pos];
            let Some(col) = prefix
                .rsplit(|c: char| !(c.is_alphanumeric() || c == '_'))
                .find(|tok| !tok.is_empty())
            else {
                continue;
            };
            let lower = col.to_ascii_lowercase();
            if lower.starts_with("is_") || lower.starts_with("has_") {
                continue;
            }
            // Ignore pseudo-column keywords like NOT, NULL, DEFAULT
            const KEYWORDS: &[&str] = &[
                "not", "null", "default", "check", "unique", "constraint", "primary", "references",
            ];
            if KEYWORDS.contains(&lower.as_str()) {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "BOOLEAN column `{col}` should start with `is_` or `has_` so call sites read as predicates."
                ),
                severity: Severity::Warning,
                span: None,
            });
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
    fn flags_bare_boolean() {
        assert_eq!(run("`active BOOLEAN NOT NULL`").len(), 1);
    }

    #[test]
    fn allows_is_prefix() {
        assert!(run("`is_active BOOLEAN NOT NULL`").is_empty());
    }

    #[test]
    fn allows_has_prefix() {
        assert!(run("`has_admin BOOLEAN NOT NULL`").is_empty());
    }
}
