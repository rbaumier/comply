use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let upper = line.to_ascii_uppercase();
            let has_col = upper.contains("IS_DELETED") || upper.contains("ISDELETED");
            let has_bool = upper.contains("BOOLEAN") || upper.contains(" BOOL ");
            if has_col && has_bool {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "`is_deleted BOOLEAN` loses the deletion time — use `deleted_at TIMESTAMPTZ NULL` instead.".into(),
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
    fn flags_is_deleted_boolean() {
        assert_eq!(
            run("`is_deleted BOOLEAN NOT NULL DEFAULT false`").len(),
            1
        );
    }

    #[test]
    fn allows_deleted_at_timestamptz() {
        assert!(run("`deleted_at TIMESTAMPTZ NULL`").is_empty());
    }
}
