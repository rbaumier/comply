use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `{n,n}` quantifiers where both numbers are the same.
fn has_useless_two_nums_quantifier(line: &str) -> bool {
    if !line.contains('/') && !line.contains("RegExp") {
        return false;
    }
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if bytes[i] == b'{' {
            // Extract the first number.
            let num1_start = i + 1;
            let mut j = num1_start;
            while j < len && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > num1_start && j < len && bytes[j] == b',' {
                let num1 = &line[num1_start..j];
                let num2_start = j + 1;
                let mut k = num2_start;
                while k < len && bytes[k].is_ascii_digit() {
                    k += 1;
                }
                if k > num2_start && k < len && bytes[k] == b'}' {
                    let num2 = &line[num2_start..k];
                    if num1 == num2 {
                        return true;
                    }
                }
            }
        }
        i += 1;
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
            if has_useless_two_nums_quantifier(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "regex-no-useless-two-nums-quantifier".into(),
                    message: "Redundant quantifier `{n,n}` — simplify to `{n}`.".into(),
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
    fn flags_same_min_max() {
        assert_eq!(run("const re = /a{3,3}/;").len(), 1);
    }

    #[test]
    fn flags_same_min_max_large() {
        assert_eq!(run("const re = /x{10,10}/;").len(), 1);
    }

    #[test]
    fn allows_different_min_max() {
        assert!(run("const re = /a{1,3}/;").is_empty());
    }

    #[test]
    fn allows_single_quantifier() {
        assert!(run("const re = /a{3}/;").is_empty());
    }
}
