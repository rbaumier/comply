use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `[]` (empty character class) inside a regex literal or `new RegExp(...)`.
fn has_empty_char_class(line: &str) -> bool {
    // Match /[]/flags
    if let Some(pos) = line.find("/[]/") {
        // Make sure it's not escaped
        let before = &line[..pos];
        let backslashes = before.chars().rev().take_while(|&c| c == '\\').count();
        if backslashes % 2 == 0 {
            return true;
        }
    }
    // Match new RegExp("[]") or new RegExp('[]')
    line.contains("RegExp(\"[]\")") || line.contains("RegExp('[]')")
        || line.contains("RegExp(\"[\\\\]\")")
        // Match Rust Regex::new("[]") or Regex::new(r"[]")
        || line.contains("Regex::new(\"[]\")") || line.contains("Regex::new(r\"[]\")")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_empty_char_class(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "regex-no-empty-character-class".into(),
                    message: "Empty character class `[]` matches nothing — add characters or remove it.".into(),
                    severity: Severity::Error,
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
    fn flags_empty_char_class_in_literal() {
        assert_eq!(run("const re = /[]/g;").len(), 1);
    }

    #[test]
    fn flags_empty_char_class_in_regexp() {
        assert_eq!(run("const re = new RegExp(\"[]\");").len(), 1);
    }

    #[test]
    fn allows_non_empty_char_class() {
        assert!(run("const re = /[a-z]/;").is_empty());
    }

    #[test]
    fn allows_bracket_in_string() {
        assert!(run("const s = \"no regex here\";").is_empty());
    }
}
