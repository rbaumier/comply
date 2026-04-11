use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const THRESHOLD: usize = 4;

/// Count logical/conditional operators on a line: `&&`, `||`, `??`, `?` (ternary).
///
/// We skip `?` that is part of `?.` (optional chaining) or `??` (nullish coalescing,
/// counted separately).
#[allow(clippy::if_same_then_else)]
fn count_operators(line: &str) -> usize {
    let mut count = 0;
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    // Skip lines that are comments
    let trimmed = line.trim();
    if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
        return 0;
    }

    while i < len {
        if i + 1 < len && bytes[i] == b'&' && bytes[i + 1] == b'&' {
            count += 1;
            i += 2;
        } else if i + 1 < len && bytes[i] == b'|' && bytes[i + 1] == b'|' {
            count += 1;
            i += 2;
        } else if i + 1 < len && bytes[i] == b'?' && bytes[i + 1] == b'?' {
            count += 1;
            i += 2;
        } else if bytes[i] == b'?' {
            // Ternary `?` — skip optional chaining `?.`
            if i + 1 < len && bytes[i + 1] == b'.' {
                // Check it's not `?.` followed by a digit (that would be `? .5` numeric)
                // but `?.` is optional chaining, skip it
                i += 2;
            } else {
                count += 1;
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    count
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if count_operators(line) >= THRESHOLD {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "expression-complexity".into(),
                    message: format!(
                        "Expression has {THRESHOLD}+ logical/conditional operators — extract to named variables."
                    ),
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
    fn flags_line_with_four_operators() {
        let src = "const x = a && b || c ?? d ? e : f;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_line_with_many_and() {
        let src = "if (a && b && c && d && e) {";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_three_operators() {
        let src = "const x = a && b || c ? d : e;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_optional_chaining() {
        // `?.` should not count — only 2 real operators here (&&, ||)
        let src = "const x = a?.b && c?.d || e;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_comments() {
        let src = "// a && b || c ?? d ? e : f";
        assert!(run(src).is_empty());
    }
}
