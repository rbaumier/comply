use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `{0}` or `{0,0}` quantifiers in regex.
fn has_zero_quantifier(line: &str) -> bool {
    if !line.contains('/') && !line.contains("RegExp") && !line.contains("Regex::") {
        return false;
    }
    line.contains("{0}") || line.contains("{0,0}")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            if has_zero_quantifier(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "regex-no-zero-quantifier".into(),
                    message: "Zero quantifier `{0}` or `{0,0}` matches nothing — remove or fix the quantifier.".into(),
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
    fn flags_zero_quantifier() {
        assert_eq!(run("const re = /a{0}/;").len(), 1);
    }

    #[test]
    fn flags_zero_zero_quantifier() {
        assert_eq!(run("const re = /a{0,0}/;").len(), 1);
    }

    #[test]
    fn allows_positive_quantifier() {
        assert!(run("const re = /a{1}/;").is_empty());
    }

    #[test]
    fn allows_range_quantifier() {
        assert!(run("const re = /a{0,1}/;").is_empty());
    }
}
