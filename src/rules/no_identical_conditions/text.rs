use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Extract the condition string from an `if (...)` or `else if (...)` clause.
/// Returns `None` when the line doesn't contain such a pattern.
fn extract_condition(line: &str) -> Option<&str> {
    let trimmed = line.trim();

    // Match `if (` or `} else if (` / `else if (`
    let rest = if let Some(r) = trimmed.strip_prefix("if") {
        r
    } else if let Some(idx) = trimmed.find("else if") {
        &trimmed[idx + 7..]
    } else {
        return None;
    };

    let rest = rest.trim_start();
    if !rest.starts_with('(') {
        return None;
    }

    // Find matching closing paren (handle nested parens).
    let bytes = rest.as_bytes();
    let mut depth = 0i32;
    let mut end = None;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }

    let close = end?;
    // Condition is between the first `(` and matching `)`.
    Some(rest[1..close].trim())
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut conditions: Vec<(String, usize)> = Vec::new();
        let mut in_chain = false;

        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();

            if let Some(cond) = extract_condition(trimmed) {
                let is_else_if = trimmed.contains("else if");

                if is_else_if && in_chain {
                    // Check for duplicate in current chain.
                    for (prev_cond, _) in &conditions {
                        if prev_cond == cond {
                            diagnostics.push(Diagnostic {
                                path: ctx.path.to_path_buf(),
                                line: idx + 1,
                                column: 1,
                                rule_id: "no-identical-conditions".into(),
                                message: format!(
                                    "Duplicate condition `{}` in if/else-if chain.",
                                    cond
                                ),
                                severity: Severity::Error,
                            });
                            break;
                        }
                    }
                    conditions.push((cond.to_string(), idx));
                } else {
                    // Start of a new chain.
                    conditions.clear();
                    conditions.push((cond.to_string(), idx));
                    in_chain = true;
                }
            } else if in_chain
                && !trimmed.is_empty()
                && !trimmed.starts_with('}')
                && !trimmed.starts_with('{')
                && !trimmed.starts_with("//")
                && !trimmed.starts_with("else")
            {
                // Non-control-flow line breaks the chain tracking.
                // We keep the chain alive across `}`, `{`, `else {`, and comments.
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
    fn flags_duplicate_condition() {
        let src = "\
if (x > 0) {
  doA();
} else if (x > 0) {
  doB();
}";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].line, 3);
    }

    #[test]
    fn allows_different_conditions() {
        let src = "\
if (x > 0) {
  doA();
} else if (x < 0) {
  doB();
}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_multiple_duplicates() {
        let src = "\
if (a === 1) {
  x();
} else if (b === 2) {
  y();
} else if (a === 1) {
  z();
} else if (b === 2) {
  w();
}";
        assert_eq!(run(src).len(), 2);
    }
}
