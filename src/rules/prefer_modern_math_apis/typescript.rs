use crate::diagnostic::{Diagnostic, Severity};

/// Detects legacy math patterns that have modern replacements.
fn find_modern_math_violation(line: &str) -> Option<&'static str> {
    if line.contains("Math.log(") && line.contains("Math.LN2") {
        return Some("Prefer `Math.log2(x)` over `Math.log(x) / Math.LN2`.");
    }
    if line.contains("Math.log(") && line.contains("Math.LN10") {
        return Some("Prefer `Math.log10(x)` over `Math.log(x) / Math.LN10`.");
    }

    if line.contains("Math.log(") && line.contains("Math.LOG2E") {
        return Some("Prefer `Math.log2(x)` over `Math.log(x) * Math.LOG2E`.");
    }
    if line.contains("Math.log(") && line.contains("Math.LOG10E") {
        return Some("Prefer `Math.log10(x)` over `Math.log(x) * Math.LOG10E`.");
    }

    if line.contains("Math.sqrt(") {
        let after = line.split("Math.sqrt(").nth(1).unwrap_or("");
        let has_pow = after.contains("** 2") || after.contains("**2");
        let has_mul_plus = after.contains(" * ") && after.contains(" + ");
        if has_pow || has_mul_plus {
            return Some("Prefer `Math.hypot(a, b)` over `Math.sqrt(a**2 + b**2)`.");
        }
    }

    None
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }
    let src = std::str::from_utf8(source).unwrap_or("");
    for (idx, line) in src.lines().enumerate() {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_log_div_ln2() {
        let d = run_ts("const x = Math.log(n) / Math.LN2;");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-modern-math-apis");
    }

    #[test]
    fn flags_log_div_ln10() {
        let d = run_ts("const x = Math.log(n) / Math.LN10;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_sqrt_sum_of_squares() {
        let d = run_ts("const h = Math.sqrt(a ** 2 + b ** 2);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_math_log2() {
        assert!(run_ts("const x = Math.log2(n);").is_empty());
    }

    #[test]
    fn allows_math_hypot() {
        assert!(run_ts("const h = Math.hypot(a, b);").is_empty());
    }

    #[test]
    fn allows_plain_math_sqrt() {
        assert!(run_ts("const r = Math.sqrt(x);").is_empty());
    }
}
