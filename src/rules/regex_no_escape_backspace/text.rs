use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `[\b]` inside regex — backspace escape inside character class.
/// Also matches `[\\b]` as it appears in `new RegExp("...")` string literals.
fn has_backspace_in_char_class(line: &str) -> bool {
    if !line.contains('/') && !line.contains("RegExp") {
        return false;
    }
    line.contains("[\\b]") || line.contains("[\\\\b]")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            if has_backspace_in_char_class(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "regex-no-escape-backspace".into(),
                    message: "`[\\b]` matches backspace, not a word boundary — use `\\b` outside a character class for word boundaries.".into(),
                    severity: Severity::Warning,
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
    fn flags_backspace_in_char_class() {
        assert_eq!(run(r#"const re = /[\b]/;"#).len(), 1);
    }

    #[test]
    fn flags_in_regexp_constructor() {
        assert_eq!(run(r#"const re = new RegExp("[\\b]");"#).len(), 1);
    }

    #[test]
    fn allows_word_boundary() {
        assert!(run(r#"const re = /\bfoo\b/;"#).is_empty());
    }
}
