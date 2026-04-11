use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const EMPTY_LOOKAROUNDS: &[&str] = &["(?=)", "(?!)", "(?<=)", "(?<!)"];

fn has_empty_lookaround(line: &str) -> bool {
    if !line.contains('/') && !line.contains("RegExp") && !line.contains("Regex::") {
        return false;
    }
    for pattern in EMPTY_LOOKAROUNDS {
        if line.contains(pattern) {
            return true;
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            if has_empty_lookaround(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "regex-no-empty-lookaround".into(),
                    message: "Empty lookaround always matches or always fails — add a pattern or remove it.".into(),
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
    fn flags_empty_lookahead() {
        assert_eq!(run("const re = /foo(?=)/;").len(), 1);
    }

    #[test]
    fn flags_empty_negative_lookahead() {
        assert_eq!(run("const re = /foo(?!)/;").len(), 1);
    }

    #[test]
    fn flags_empty_lookbehind() {
        assert_eq!(run("const re = /(?<=)bar/;").len(), 1);
    }

    #[test]
    fn flags_empty_negative_lookbehind() {
        assert_eq!(run("const re = /(?<!)bar/;").len(), 1);
    }

    #[test]
    fn allows_non_empty_lookahead() {
        assert!(run("const re = /foo(?=bar)/;").is_empty());
    }
}
