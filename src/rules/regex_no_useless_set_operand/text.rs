use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects useless operands in `v`-flag character class set operations.
/// Example: `[\d&&\w]` — `\d` is a subset of `\w`, so intersection is just `\d`.
/// Example: `[\w--\W]` — `\W` is the complement of `\w`, subtraction is useless.
fn find_useless_set_operands(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'/' {
            if i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b')') {
                i += 1;
                continue;
            }
            let start = i + 1;
            let mut j = start;
            while j < len {
                if bytes[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if bytes[j] == b'/' {
                    let flag_start = j + 1;
                    let mut flag_end = flag_start;
                    while flag_end < len && bytes[flag_end].is_ascii_alphabetic() {
                        flag_end += 1;
                    }
                    let flags = &line[flag_start..flag_end];
                    if flags.contains('v') {
                        let pattern = &line[start..j];
                        if has_useless_set_op(pattern) {
                            hits.push(i);
                        }
                    }
                    i = flag_end;
                    break;
                }
                j += 1;
            }
        }
        i += 1;
    }
    hits
}

fn has_useless_set_op(pattern: &str) -> bool {
    // Detect `[\d&&\w]`, `[\w&&\d]`, `[\w--\W]`, `[\D&&\d]` etc.
    let complementary_pairs: &[(&str, &str)] = &[
        (r"\d", r"\w"),  // \d is subset of \w
        (r"\w", r"\W"),  // \w -- \W = \w
        (r"\d", r"\D"),  // \d && \D = empty
        (r"\s", r"\S"),  // \s && \S = empty
    ];

    for &(a, b) in complementary_pairs {
        // Check intersection where one is subset
        let intersection = format!("[{}&&{}]", a, b);
        let intersection_rev = format!("[{}&&{}]", b, a);
        let subtraction = format!("[{}--{}]", a, b);
        let subtraction_rev = format!("[{}--{}]", b, a);

        if pattern.contains(&intersection)
            || pattern.contains(&intersection_rev)
            || pattern.contains(&subtraction)
            || pattern.contains(&subtraction_rev)
        {
            return true;
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for col in find_useless_set_operands(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "regex-no-useless-set-operand".into(),
                    message: "Useless operand in character class set operation.".into(),
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
    fn flags_subset_intersection() {
        assert_eq!(run(r#"const re = /[\d&&\w]/v;"#).len(), 1);
    }

    #[test]
    fn flags_complement_subtraction() {
        assert_eq!(run(r#"const re = /[\w--\W]/v;"#).len(), 1);
    }

    #[test]
    fn allows_non_v_flag() {
        assert!(run(r#"const re = /[\d]/g;"#).is_empty());
    }
}
