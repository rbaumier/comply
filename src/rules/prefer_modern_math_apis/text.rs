use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects legacy math patterns that have modern replacements:
/// - `Math.log(…) / Math.LN2`  → `Math.log2(…)`
/// - `Math.log(…) / Math.LN10` → `Math.log10(…)`
/// - `Math.log(…) * Math.LOG2E`  → `Math.log2(…)`
/// - `Math.log(…) * Math.LOG10E` → `Math.log10(…)`
/// - `Math.sqrt(a * a + b * b)` → `Math.hypot(a, b)`  (single-line only)
fn find_modern_math_violation(line: &str) -> Option<&'static str> {
    // Log division patterns
    if line.contains("Math.log(") && line.contains("Math.LN2") {
        return Some("Prefer `Math.log2(x)` over `Math.log(x) / Math.LN2`.");
    }
    if line.contains("Math.log(") && line.contains("Math.LN10") {
        return Some("Prefer `Math.log10(x)` over `Math.log(x) / Math.LN10`.");
    }

    // Log multiplication patterns
    if line.contains("Math.log(") && line.contains("Math.LOG2E") {
        return Some("Prefer `Math.log2(x)` over `Math.log(x) * Math.LOG2E`.");
    }
    if line.contains("Math.log(") && line.contains("Math.LOG10E") {
        return Some("Prefer `Math.log10(x)` over `Math.log(x) * Math.LOG10E`.");
    }

    // Math.sqrt with sum of squares → Math.hypot
    // Heuristic: `Math.sqrt(` on a line that also contains `*` and `+`
    // and the argument looks like a sum of squared terms.
    if line.contains("Math.sqrt(") {
        // Quick check: contains at least `** 2` or two multiplications separated by `+`
        let after = line.split("Math.sqrt(").nth(1).unwrap_or("");
        let has_pow = after.contains("** 2") || after.contains("**2");
        let has_mul_plus = after.contains(" * ") && after.contains(" + ");
        if has_pow || has_mul_plus {
            return Some("Prefer `Math.hypot(a, b)` over `Math.sqrt(a**2 + b**2)`.");
        }
    }

    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(msg) = find_modern_math_violation(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-modern-math-apis".into(),
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
    fn flags_log_div_ln2() {
        let d = run("const x = Math.log(n) / Math.LN2;");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-modern-math-apis");
    }

    #[test]
    fn flags_log_div_ln10() {
        let d = run("const x = Math.log(n) / Math.LN10;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_log_mul_log2e() {
        let d = run("const x = Math.log(n) * Math.LOG2E;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_log_mul_log10e() {
        let d = run("const x = Math.LOG10E * Math.log(n);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_sqrt_sum_of_squares_pow() {
        let d = run("const h = Math.sqrt(a ** 2 + b ** 2);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_sqrt_sum_of_squares_mul() {
        let d = run("const h = Math.sqrt(a * a + b * b);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_math_log2() {
        assert!(run("const x = Math.log2(n);").is_empty());
    }

    #[test]
    fn allows_math_hypot() {
        assert!(run("const h = Math.hypot(a, b);").is_empty());
    }

    #[test]
    fn allows_plain_math_sqrt() {
        assert!(run("const r = Math.sqrt(x);").is_empty());
    }
}
