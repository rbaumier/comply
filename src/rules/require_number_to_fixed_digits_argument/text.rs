use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// `.toFixed()` or `.toFixed(  )` — no digits argument.
fn has_empty_to_fixed(line: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = line[start..].find(".toFixed(") {
        let abs = start + pos + 9; // skip past ".toFixed("
        let rest = &line[abs..];
        let trimmed = rest.trim_start();
        if trimmed.starts_with(')') {
            return true;
        }
        start = abs;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_empty_to_fixed(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "require-number-to-fixed-digits-argument".into(),
                    message: "Missing the digits argument in `.toFixed()` — use `.toFixed(0)` explicitly.".into(),
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
    fn flags_empty_to_fixed() {
        assert_eq!(run("const s = num.toFixed();").len(), 1);
    }

    #[test]
    fn flags_to_fixed_with_whitespace() {
        assert_eq!(run("const s = num.toFixed(  );").len(), 1);
    }

    #[test]
    fn allows_to_fixed_with_digits() {
        assert!(run("const s = num.toFixed(2);").is_empty());
    }

    #[test]
    fn allows_to_fixed_with_zero() {
        assert!(run("const s = num.toFixed(0);").is_empty());
    }

    #[test]
    fn flags_chained_to_fixed() {
        assert_eq!(run("price.toFixed().padStart(5)").len(), 1);
    }
}
