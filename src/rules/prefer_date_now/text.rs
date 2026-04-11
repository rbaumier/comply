use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects patterns that should be replaced with `Date.now()`:
/// - `new Date().getTime()`
/// - `new Date().valueOf()`
/// - `+new Date()`
/// - `Number(new Date())`
fn find_date_now_violation(line: &str) -> Option<&'static str> {
    // `new Date().getTime()` or `new Date().valueOf()`
    if line.contains("new Date().getTime()")
        || line.contains("new Date().valueOf()")
    {
        return Some("Prefer `Date.now()` over `new Date().getTime()`/`.valueOf()`.");
    }

    // `+new Date()` — unary plus coercion
    if line.contains("+new Date()") {
        return Some("Prefer `Date.now()` over `+new Date()`.");
    }

    // `Number(new Date())`
    if line.contains("Number(new Date())") {
        return Some("Prefer `Date.now()` over `Number(new Date())`.");
    }

    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(msg) = find_date_now_violation(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-date-now".into(),
                    message: msg.into(),
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
    fn flags_get_time() {
        let d = run("const ts = new Date().getTime();");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-date-now");
    }

    #[test]
    fn flags_value_of() {
        let d = run("const ts = new Date().valueOf();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_unary_plus() {
        let d = run("const ts = +new Date();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_number_coercion() {
        let d = run("const ts = Number(new Date());");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_date_now() {
        assert!(run("const ts = Date.now();").is_empty());
    }

    #[test]
    fn allows_new_date_with_args() {
        assert!(run("const d = new Date(2024, 0, 1).getTime();").is_empty());
    }
}
