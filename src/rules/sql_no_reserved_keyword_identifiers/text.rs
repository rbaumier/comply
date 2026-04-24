use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const RESERVED: &[&str] = &[
    "USER", "ORDER", "GROUP", "TABLE", "SELECT", "FROM", "WHERE", "JOIN", "UNION", "GRANT",
    "REFERENCES", "CHECK", "DEFAULT", "PRIMARY", "FOREIGN", "UNIQUE", "COLUMN", "CONSTRAINT",
    "DESC", "ASC", "LIMIT", "OFFSET", "AS", "CASE", "WHEN", "END", "RETURNING", "VALUES",
];

fn extract_table_name(upper: &str, original: &str) -> Option<String> {
    let idx = upper.find("CREATE TABLE")?;
    let after = &original[idx + "CREATE TABLE".len()..];
    let after_upper = &upper[idx + "CREATE TABLE".len()..];
    let rest = if after_upper.trim_start().starts_with("IF NOT EXISTS") {
        let t = after.trim_start();
        t["IF NOT EXISTS".len()..].trim_start()
    } else {
        after.trim_start()
    };
    let mut ident = String::new();
    for ch in rest.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            ident.push(ch);
        } else {
            break;
        }
    }
    if ident.is_empty() {
        None
    } else {
        Some(ident)
    }
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let upper = line.to_ascii_uppercase();
            // Detect CREATE TABLE name
            if upper.contains("CREATE TABLE")
                && let Some(name) = extract_table_name(&upper, line)
                    && RESERVED.contains(&name.to_ascii_uppercase().as_str()) {
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: idx + 1,
                            column: 1,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "`{name}` is a PostgreSQL reserved word — rename the table."
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
            // Detect ADD COLUMN / column definitions using reserved words
            if upper.contains("ADD COLUMN ") {
                let pos = upper.find("ADD COLUMN ").unwrap();
                let after = &line[pos + "ADD COLUMN ".len()..].trim_start();
                let mut ident = String::new();
                for ch in after.chars() {
                    if ch.is_alphanumeric() || ch == '_' {
                        ident.push(ch);
                    } else {
                        break;
                    }
                }
                if !ident.is_empty()
                    && RESERVED.contains(&ident.to_ascii_uppercase().as_str())
                {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Column `{ident}` is a PostgreSQL reserved word — rename it."
                        ),
                        severity: Severity::Warning,
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

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_table_named_user() {
        assert_eq!(run("`CREATE TABLE user (id INT);`").len(), 1);
    }

    #[test]
    fn flags_add_column_order() {
        assert_eq!(run("`ALTER TABLE t ADD COLUMN order INT;`").len(), 1);
    }

    #[test]
    fn allows_non_reserved() {
        assert!(run("`CREATE TABLE account (id INT);`").is_empty());
    }
}
