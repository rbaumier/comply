//! tanstack-start-loader-stale-time backend — scans `ensureQueryData(...)`
//! calls and flags loaders whose `staleTime` is missing or below 5000ms,
//! which would cause refetching during typical navigation transitions.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

const MIN_STALE_TIME_MS: u64 = 5000;

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("ensureQueryData(") {
            return vec![];
        }

        let lines: Vec<&str> = src.lines().collect();
        let mut diags = Vec::new();
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                i += 1;
                continue;
            }
            if let Some(call_pos) = line.find("ensureQueryData(") {
                // Collect the call body starting at '(' until balanced ')' or 20 lines.
                let start_col = call_pos + "ensureQueryData".len();
                let mut buf = String::new();
                let mut depth: i32 = 0;
                let mut closed = false;
                let mut consumed_lines = 0;
                let mut j = i;
                let mut first = true;
                while j < lines.len() && consumed_lines < 20 {
                    let segment = if first {
                        first = false;
                        &lines[j][start_col..]
                    } else {
                        lines[j]
                    };
                    for ch in segment.chars() {
                        buf.push(ch);
                        if ch == '(' {
                            depth += 1;
                        } else if ch == ')' {
                            depth -= 1;
                            if depth == 0 {
                                closed = true;
                                break;
                            }
                        }
                    }
                    if closed {
                        break;
                    }
                    buf.push('\n');
                    j += 1;
                    consumed_lines += 1;
                }

                if closed {
                    let line_num = i + 1;
                    let col = call_pos + 1;
                    if let Some(stale_pos) = buf.find("staleTime:") {
                        let after = &buf[stale_pos + "staleTime:".len()..];
                        let after_trim = after.trim_start();
                        let digits: String = after_trim
                            .chars()
                            .take_while(|c| c.is_ascii_digit())
                            .collect();
                        if let Ok(n) = digits.parse::<u64>()
                            && n < MIN_STALE_TIME_MS
                        {
                            diags.push(Diagnostic {
                                path: ctx.path.to_path_buf(),
                                line: line_num,
                                column: col,
                                rule_id: super::META.id.into(),
                                message: format!(
                                    "`staleTime: {n}` is below {MIN_STALE_TIME_MS}ms — loader data will refetch during navigation."
                                ),
                                severity: Severity::Warning,
                                span: None,
                            });
                        }
                    } else {
                        diags.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: line_num,
                            column: col,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "`ensureQueryData` call is missing `staleTime` — set it to at least {MIN_STALE_TIME_MS}ms to avoid refetches during navigation."
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }

                // Advance past the processed block.
                i = j + 1;
                continue;
            }
            i += 1;
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), src))
    }

    #[test]
    fn flags_stale_time_below_threshold() {
        let src = r#"loader: () => ensureQueryData({ queryKey: ['x'], queryFn: f, staleTime: 1000 })"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_missing_stale_time() {
        let src = r#"loader: () => ensureQueryData({ queryKey: ['x'], queryFn: f })"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_stale_time_at_threshold() {
        let src = r#"loader: () => ensureQueryData({ queryKey: ['x'], queryFn: f, staleTime: 5000 })"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_stale_time_above_threshold() {
        let src = r#"loader: () => ensureQueryData({ queryKey: ['x'], queryFn: f, staleTime: 30000 })"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_commented_line() {
        let src = "// ensureQueryData({ queryKey: ['x'], queryFn: f })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn handles_multiline_call() {
        let src = r#"
            ensureQueryData({
              queryKey: ['users'],
              queryFn: fetchUsers,
              staleTime: 2000,
            })
        "#;
        assert_eq!(run(src).len(), 1);
    }
}
