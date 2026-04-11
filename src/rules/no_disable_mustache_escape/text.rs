use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const PATTERNS: &[&str] = &[
    "escapeMarkup = false",
    "escapeMarkup: false",
    "escape = false",
    "noEscape: true",
    "noEscape = true",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            for pattern in PATTERNS {
                if trimmed.contains(pattern) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "no-disable-mustache-escape".into(),
                        message: format!(
                            "Disabling HTML escaping via `{}` — keep escaping enabled to prevent XSS.",
                            pattern,
                        ),
                        severity: Severity::Error,
                    });
                    break;
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
    fn flags_escape_markup_false() {
        assert_eq!(run("options.escapeMarkup = false;").len(), 1);
    }

    #[test]
    fn flags_escape_markup_property() {
        assert_eq!(run("{ escapeMarkup: false }").len(), 1);
    }

    #[test]
    fn flags_no_escape_true() {
        assert_eq!(run("{ noEscape: true }").len(), 1);
    }

    #[test]
    fn allows_escape_enabled() {
        assert!(run("{ escapeMarkup: true }").is_empty());
    }
}
