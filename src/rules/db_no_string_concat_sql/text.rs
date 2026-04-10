use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const SQL_KEYWORDS: &[&str] = &["SELECT ", "INSERT ", "UPDATE ", "DELETE ", "WHERE ", "FROM "];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let upper = line.to_ascii_uppercase();
            let has_sql = SQL_KEYWORDS.iter().any(|k| upper.contains(k));
            if !has_sql { continue; }
            // Detect string concat: `+` or `${}` interpolation with a variable
            if (line.contains("+ ") || line.contains(" +")) && !line.contains("$1") && !line.contains("$2")
                && (line.contains("${") || line.contains("\" +") || line.contains("' +"))
            {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(), line: idx + 1, column: 1,
                    rule_id: "db-no-string-concat-sql".into(),
                    message: "String concatenation with SQL keywords — SQL injection risk. Use parameterized queries (`$1`, `?`) instead.".into(),
                    severity: Severity::Error,
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
    fn run(source: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("t.ts"), source)) }

    #[test]
    fn flags_concat() { assert_eq!(run("const q = \"SELECT * FROM users WHERE id = \" + userId;").len(), 1); }
    #[test]
    fn allows_param() { assert!(run("const q = \"SELECT * FROM users WHERE id = $1\";").is_empty()); }
}
