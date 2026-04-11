use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Returns true if arguments contain `expected` before `actual`.
fn has_expected_before_actual(line: &str) -> bool {
    if let Some(exp_pos) = line.find("expected")
        && let Some(act_pos) = line.find("actual") {
            return exp_pos < act_pos;
        }
    false
}

/// Returns true if arguments contain `max` before `min`.
fn has_max_before_min(line: &str) -> bool {
    if let Some(max_pos) = line.find("max")
        && let Some(min_pos) = line.find("min") {
            return max_pos < min_pos;
        }
    false
}

/// Returns true if the line looks like a function call (has parens wrapping the suspicious args).
fn is_call_with_args(line: &str) -> bool {
    line.contains('(') && line.contains(')')
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if !is_call_with_args(line) {
                continue;
            }
            if has_expected_before_actual(line) || has_max_before_min(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "arguments-order".into(),
                    message: "Arguments appear to be in the wrong order — `expected` should come after `actual`, `min` before `max`.".into(),
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
    fn flags_expected_before_actual() {
        assert_eq!(run("assertEqual(expected, actual);").len(), 1);
    }

    #[test]
    fn flags_max_before_min() {
        assert_eq!(run("clamp(max, min);").len(), 1);
    }

    #[test]
    fn allows_correct_order_actual_expected() {
        assert!(run("assertEqual(actual, expected);").is_empty());
    }

    #[test]
    fn allows_correct_order_min_max() {
        assert!(run("clamp(min, max);").is_empty());
    }

    #[test]
    fn ignores_non_call_lines() {
        assert!(run("// expected before actual").is_empty());
    }
}
