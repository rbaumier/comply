use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if let Some(pos) = t.find("sql.raw(") {
                let after_paren = &t[pos + 8..];
                let is_string_literal =
                    after_paren.starts_with('"') || after_paren.starts_with('\'');
                if !is_string_literal {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "drizzle-no-sql-raw-with-variable".into(),
                        message: "`sql.raw()` with a non-literal argument is a SQL injection vector — use `sql` tagged templates with parameterized values instead.".into(),
                        severity: Severity::Error,
                        span: None,
                    });
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
    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), s))
    }
    #[test]
    fn flags_variable_argument() {
        assert_eq!(run("sql.raw(userInput)").len(), 1);
    }
    #[test]
    fn flags_template_literal() {
        assert_eq!(run("sql.raw(`SELECT * FROM ${tableName}`)").len(), 1);
    }
    #[test]
    fn allows_string_literal_double_quote() {
        assert!(run("sql.raw(\"SELECT 1\")").is_empty());
    }
    #[test]
    fn allows_string_literal_single_quote() {
        assert!(run("sql.raw('NOW()')").is_empty());
    }
    #[test]
    fn allows_tagged_template() {
        assert!(run("sql`WHERE id = ${userId}`").is_empty());
    }
}
