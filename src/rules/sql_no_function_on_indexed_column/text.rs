use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const BAD_FUNCS: &[&str] = &[
    "DATE_TRUNC(",
    "LOWER(",
    "UPPER(",
    "COALESCE(",
    "EXTRACT(",
    "CAST(",
    "TO_CHAR(",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let upper = line.to_ascii_uppercase();
            if !upper.contains("WHERE") {
                continue;
            }
            // Only consider content after WHERE
            let Some(where_pos) = upper.find("WHERE") else {
                continue;
            };
            let after = &upper[where_pos..];
            for func in BAD_FUNCS {
                if after.contains(func) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`{}` in WHERE defeats the index — normalize the column or add a functional index.",
                            func.trim_end_matches('(')
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                    break;
                }
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
    fn flags_date_trunc() {
        assert_eq!(
            run("`SELECT id FROM log WHERE date_trunc('day', created_at) = '2024-01-01'`").len(),
            1
        );
    }

    #[test]
    fn flags_lower() {
        assert_eq!(
            run("`SELECT id FROM user WHERE LOWER(email) = 'a@b.c'`").len(),
            1
        );
    }

    #[test]
    fn allows_plain_column_comparison() {
        assert!(run("`SELECT id FROM user WHERE email = 'a@b.c'`").is_empty());
    }
}
