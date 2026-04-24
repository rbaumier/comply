use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        // The rule flags DROP COLUMN unless the file contains an explicit
        // marker/comment that the column was already deprecated in a prior
        // release. Accept either a SQL comment or a plain code comment with
        // "deprecated" or "expand-contract" on the nearby lines.
        let lower = ctx.source.to_ascii_lowercase();
        let file_marks_deprecation = lower.contains("deprecated in")
            || lower.contains("expand-contract")
            || lower.contains("unused since");
        for (idx, line) in ctx.source.lines().enumerate() {
            let upper = line.to_ascii_uppercase();
            if !(upper.contains("DROP COLUMN") && upper.contains("ALTER TABLE")) {
                continue;
            }
            if file_marks_deprecation {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: "DROP COLUMN without a prior deprecation release breaks running deploys — deprecate first, drop later.".into(),
                severity: Severity::Warning,
                span: None,
            });
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
    fn flags_bare_drop_column() {
        assert_eq!(
            run("`ALTER TABLE account DROP COLUMN legacy_flag;`").len(),
            1
        );
    }

    #[test]
    fn allows_with_deprecation_marker() {
        let src = "// legacy_flag deprecated in v4.2 — expand-contract complete\n`ALTER TABLE account DROP COLUMN legacy_flag;`";
        assert!(run(src).is_empty());
    }
}
