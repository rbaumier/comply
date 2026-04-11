use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_loop_start(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("for ")
        || trimmed.starts_with("for(")
        || trimmed.starts_with("while ")
        || trimmed.starts_with("while(")
        || trimmed.starts_with("do ")
        || trimmed.starts_with("do{")
        || trimmed == "do"
}

fn is_break_or_continue(line: &str) -> bool {
    let trimmed = line.trim();
    has_keyword(trimmed, "break") || has_keyword(trimmed, "continue")
}

/// Check if `keyword` appears as a standalone keyword (not part of a larger identifier).
fn has_keyword(s: &str, keyword: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = s[start..].find(keyword) {
        let abs = start + pos;
        let end = abs + keyword.len();
        let before_ok =
            abs == 0 || !s.as_bytes()[abs - 1].is_ascii_alphanumeric() && s.as_bytes()[abs - 1] != b'_';
        let after_ok = end >= s.len()
            || (!s.as_bytes()[end].is_ascii_alphanumeric() && s.as_bytes()[end] != b'_');
        if before_ok && after_ok {
            return true;
        }
        start = abs + 1;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            if is_loop_start(lines[i]) {
                let loop_line = i;
                let mut depth: i32 = 0;
                let mut body_started = false;
                let mut bc_count = 0;
                // Count braces on the loop line
                for ch in lines[i].chars() {
                    if ch == '{' {
                        depth += 1;
                        body_started = true;
                    } else if ch == '}' {
                        depth -= 1;
                    }
                }
                let mut j = i + 1;
                while j < lines.len() {
                    for ch in lines[j].chars() {
                        if ch == '{' {
                            depth += 1;
                            body_started = true;
                        } else if ch == '}' {
                            depth -= 1;
                        }
                    }
                    if body_started && depth <= 0 {
                        break;
                    }
                    if is_break_or_continue(lines[j]) {
                        bc_count += 1;
                    }
                    j += 1;
                }
                if bc_count >= 2 {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: loop_line + 1,
                        column: 1,
                        rule_id: "too-many-break-or-continue".into(),
                        message: format!(
                            "Loop contains {bc_count} `break`/`continue` statements — consider refactoring."
                        ),
                        severity: Severity::Warning,
                    });
                }
            }
            i += 1;
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
    fn flags_two_breaks() {
        let src = "for (const x of arr) {\n  if (a) break;\n  if (b) break;\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_break_and_continue() {
        let src = "while (true) {\n  if (a) continue;\n  if (b) break;\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_single_break() {
        let src = "for (const x of arr) {\n  if (a) break;\n  doWork();\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_no_break() {
        let src = "for (const x of arr) {\n  doWork(x);\n}";
        assert!(run(src).is_empty());
    }
}
