use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("new Pool(") && !src.contains("drizzle(") && !src.contains("createPool(") {
            return vec![];
        }
        if src.contains("statement_timeout") {
            return vec![];
        }

        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            let t = line.trim();
            if t.starts_with("new Pool(") || t.contains("= new Pool(") || t.contains("drizzle(") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "DB pool config is missing `statement_timeout` — add it to prevent runaway queries.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
                break;
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("db.ts"), src))
    }

    #[test]
    fn flags_pool_without_timeout() {
        assert_eq!(
            run("const pool = new Pool({ connectionString: url })").len(),
            1
        );
    }

    #[test]
    fn allows_pool_with_timeout() {
        assert!(
            run("const pool = new Pool({ connectionString: url, statement_timeout: '30s' })")
                .is_empty()
        );
    }

    #[test]
    fn ignores_non_pool_files() {
        assert!(run("const x = 1;").is_empty());
    }
}
