use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();
        let len = lines.len();

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Look for `else if` — then check whether an `else {` eventually follows.
            if !contains_else_if(trimmed) {
                continue;
            }

            // Walk forward past this `else if` block to see if the chain
            // continues with another `else if` or ends with `else {`.
            // We only flag the *last* `else if` in a chain that lacks `else`.
            if has_else_following(idx, &lines, len) {
                continue;
            }

            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "elseif-without-else".into(),
                message: "`if/else if` chain without a final `else` — add an `else` block to handle remaining cases.".into(),
                severity: Severity::Warning,
            });
        }

        diagnostics
    }
}

fn contains_else_if(trimmed: &str) -> bool {
    // Match `} else if`, `else if`, as the start of a line.
    if trimmed.starts_with("else if") || trimmed.starts_with("} else if") {
        return true;
    }
    // Also match inline: `} else if (`
    trimmed.contains("} else if") || trimmed.contains("else if (") || trimmed.contains("else if(")
}

/// Walk forward from an `else if` line, skip its brace-delimited body,
/// then check whether the next non-blank token is `else`.
fn has_else_following(start: usize, lines: &[&str], len: usize) -> bool {
    // We need to find the opening `{` of this else-if block, skipping any
    // leading `}` that closes the previous branch.  Track depth starting
    // from the *first* `{` we encounter (ignore braces before that).
    let mut depth: i32 = 0;
    let mut found_open = false;
    let mut i = start;

    while i < len {
        let mut closed_on_this_line = false;
        for ch in lines[i].chars() {
            if ch == '{' {
                if !found_open {
                    found_open = true;
                    depth = 1;
                } else if !closed_on_this_line {
                    depth += 1;
                }
            } else if ch == '}' && found_open && !closed_on_this_line {
                depth -= 1;
                if depth == 0 {
                    closed_on_this_line = true;
                }
            }
        }
        if closed_on_this_line {
            // We've closed the else-if block on this line.
            let after_close = lines[i].trim();
            if after_close.contains("} else if") || after_close.contains("} else {") || after_close.ends_with("} else") {
                return true;
            }
            // Check next non-blank line
            let mut j = i + 1;
            while j < len && lines[j].trim().is_empty() {
                j += 1;
            }
            if j < len {
                let next = lines[j].trim();
                if next.starts_with("else") {
                    return true;
                }
            }
            return false;
        }
        i += 1;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_else_if_without_else() {
        let src = r#"
if (a) {
  doA();
} else if (b) {
  doB();
}
"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "elseif-without-else");
    }

    #[test]
    fn allows_else_if_with_else() {
        let src = r#"
if (a) {
  doA();
} else if (b) {
  doB();
} else {
  doC();
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_chained_else_if_without_final_else() {
        let src = r#"
if (a) {
  doA();
} else if (b) {
  doB();
} else if (c) {
  doC();
}
"#;
        let d = run(src);
        // Only the last `else if` in the chain should be flagged.
        assert_eq!(d.len(), 1);
        assert!(d[0].line > 2);
    }

    #[test]
    fn allows_plain_if_without_else() {
        let src = r#"
if (a) {
  doA();
}
"#;
        // A bare `if` without `else if` is fine — not a chain.
        assert!(run(src).is_empty());
    }
}
