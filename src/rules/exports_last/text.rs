//! exports-last backend — flag exports that precede non-export code.
//!
//! Heuristic: after stripping blank lines and comments, the "trailing
//! block" of the file must be the only place exports live. Any `export`
//! line followed by a later non-export, non-comment, non-blank line is
//! flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_export_line(trimmed: &str) -> bool {
    // Treat `export` and `export default …` as exports; re-exports too.
    trimmed.starts_with("export ")
        || trimmed.starts_with("export{")
        || trimmed == "export"
        || trimmed.starts_with("export*")
}

fn is_comment_or_blank(trimmed: &str) -> bool {
    trimmed.is_empty()
        || trimmed.starts_with("//")
        || trimmed.starts_with("/*")
        || trimmed.starts_with("*")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let lines: Vec<&str> = ctx.source.lines().collect();
        if lines.is_empty() {
            return Vec::new();
        }

        // Collect the 1-based line number of every export line.
        let export_lines: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| is_export_line(l.trim_start()))
            .map(|(i, _)| i + 1)
            .collect();

        if export_lines.is_empty() {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for &line_no in &export_lines {
            // Look at every following line — if any is non-export, non-blank,
            // non-comment code, this export is not at the end.
            let mut has_code_after = false;
            for after in lines.iter().skip(line_no) {
                let trimmed = after.trim_start();
                if is_comment_or_blank(trimmed) {
                    continue;
                }
                if is_export_line(trimmed) {
                    continue;
                }
                has_code_after = true;
                break;
            }
            if has_code_after {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: line_no,
                    column: 1,
                    rule_id: "exports-last".into(),
                    message: "Export statement is not at the end of the file.".into(),
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
    fn flags_export_before_code() {
        let src = "export const x = 1;\nconst y = 2;\n";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 1);
    }

    #[test]
    fn allows_all_exports_at_end() {
        let src = "const y = 2;\nexport const x = 1;\nexport const z = 3;\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_file_with_no_exports() {
        assert!(run("const x = 1;\n").is_empty());
    }

    #[test]
    fn allows_comment_after_exports() {
        let src = "const y = 2;\nexport const x = 1;\n// tail comment\n";
        assert!(run(src).is_empty());
    }
}
