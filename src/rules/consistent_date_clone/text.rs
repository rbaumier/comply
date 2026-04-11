use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `new Date(expr.getTime())` or `new Date(expr.valueOf())` patterns.
fn has_date_clone_via_gettime(line: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = line[start..].find("new Date(") {
        let abs = start + pos + 9; // skip past "new Date("
        let rest = &line[abs..];

        // Look for `.getTime())` or `.valueOf())` — the closing paren of the
        // inner call followed immediately by the closing paren of `new Date(`.
        if rest.contains(".getTime())") || rest.contains(".valueOf())") {
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
            if has_date_clone_via_gettime(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "consistent-date-clone".into(),
                    message:
                        "Unnecessary `.getTime()`/`.valueOf()` — use `new Date(date)` directly."
                            .into(),
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
    fn flags_gettime() {
        let d = run("const clone = new Date(d.getTime());");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "consistent-date-clone");
    }

    #[test]
    fn flags_valueof() {
        let d = run("const clone = new Date(d.valueOf());");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_direct_clone() {
        assert!(run("const clone = new Date(d);").is_empty());
    }

    #[test]
    fn allows_date_with_number() {
        assert!(run("const d = new Date(1234567890);").is_empty());
    }

    #[test]
    fn allows_date_now() {
        assert!(run("const d = new Date(Date.now());").is_empty());
    }
}
