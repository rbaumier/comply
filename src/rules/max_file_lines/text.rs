//! max-file-lines backend — plain line count, same impl for every language.
//!
//! No AST required; we just count newlines in the source. The check applies
//! identically to TS, TSX, JS, and Rust — the rule's job is to enforce a
//! ceiling on file size regardless of syntax.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

/// Default cap: 200 lines ≈ one screen at 12pt — forces splitting
/// before a file owns more than one concern. The user can override
/// in `comply.toml` via `[rules.max-file-lines] max = N`.
pub const DEFAULT_MAX_LINES: usize = 200;

pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let max_lines = ctx
            .config
            .threshold("max-file-lines", "max", DEFAULT_MAX_LINES);
        let count = ctx.source.lines().count();
        if count <= max_lines {
            return vec![];
        }
        vec![Diagnostic {
            path: ctx.path.to_path_buf(),
            line: max_lines + 1,
            column: 1,
            rule_id: "max-file-lines".into(),
            message: format!(
                "File has {count} lines — split by responsibility (max {max_lines}). \
                 Extract helpers below line {max_lines} into a separate module."
            ),
            severity: Severity::Error,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("foo.ts"), source))
    }

    #[test]
    fn flags_file_over_limit() {
        let source = "x\n".repeat(DEFAULT_MAX_LINES + 5);
        let diags = run(&source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "max-file-lines");
    }

    #[test]
    fn allows_file_at_limit() {
        assert!(run(&"x\n".repeat(DEFAULT_MAX_LINES)).is_empty());
    }

    #[test]
    fn allows_file_under_limit() {
        assert!(run(&"x\n".repeat(50)).is_empty());
    }
}
