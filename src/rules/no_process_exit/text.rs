use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// True if the line contains `process.exit(` as a standalone call.
fn has_process_exit(line: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = line[start..].find("process.exit(") {
        let abs = start + pos;
        // Make sure it's not part of a larger identifier (e.g., `myprocess.exit(`)
        if abs == 0 {
            return true;
        }
        let prev = line.as_bytes()[abs - 1];
        if !prev.is_ascii_alphanumeric() && prev != b'_' && prev != b'.' {
            return true;
        }
        start = abs + 13;
    }
    false
}

/// True if the first line is a shebang (`#!/usr/bin/env node`, etc.)
fn has_shebang(source: &str) -> bool {
    source.starts_with("#!")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if has_shebang(ctx.source) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            if has_process_exit(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-process-exit".into(),
                    message: "`process.exit()` terminates abruptly — throw an error instead."
                        .into(),
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
    fn flags_process_exit() {
        assert_eq!(run("process.exit(1);").len(), 1);
    }

    #[test]
    fn flags_process_exit_no_args() {
        assert_eq!(run("process.exit();").len(), 1);
    }

    #[test]
    fn allows_shebang_file() {
        assert!(run("#!/usr/bin/env node\nprocess.exit(1);").is_empty());
    }

    #[test]
    fn allows_comment() {
        assert!(run("// process.exit(1);").is_empty());
    }

    #[test]
    fn flags_in_conditional() {
        assert_eq!(run("if (err) process.exit(1);").len(), 1);
    }

    #[test]
    fn does_not_flag_substring() {
        // `myprocess.exit(` should not be flagged — the `m` before `process` is alphanumeric
        assert!(run("myprocess.exit(1);").is_empty());
    }
}
