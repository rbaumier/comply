use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Extract the table identifier after CREATE TABLE [IF NOT EXISTS].
fn extract_table_name(upper: &str, original: &str) -> Option<String> {
    let idx = upper.find("CREATE TABLE")?;
    let after = &original[idx + "CREATE TABLE".len()..];
    let after_upper = &upper[idx + "CREATE TABLE".len()..];
    // Skip IF NOT EXISTS if present
    let trimmed = after.trim_start();
    let trimmed_upper = after_upper.trim_start();
    let rest = if trimmed_upper.starts_with("IF NOT EXISTS") {
        trimmed["IF NOT EXISTS".len()..].trim_start()
    } else {
        trimmed
    };
    // Take first identifier token
    let mut ident = String::new();
    for ch in rest.chars() {
        if ch.is_alphanumeric() || ch == '_' || ch == '.' || ch == '"' {
            ident.push(ch);
        } else {
            break;
        }
    }
    // Strip schema prefix and quotes
    let cleaned = ident.replace('"', "");
    let name = cleaned.rsplit('.').next().unwrap_or(&cleaned).to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn looks_plural(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower.len() < 3 {
        return false;
    }
    // Common singular exceptions ending in 's'
    const EXCEPTIONS: &[&str] = &["status", "address", "business", "progress", "analysis"];
    if EXCEPTIONS.iter().any(|e| lower == *e) {
        return false;
    }
    lower.ends_with('s') && !lower.ends_with("ss") && !lower.ends_with("us")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let upper = line.to_ascii_uppercase();
            if !upper.contains("CREATE TABLE") {
                continue;
            }
            if let Some(name) = extract_table_name(&upper, line)
                && looks_plural(&name) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Table `{name}` appears plural — use singular (one row = one entity)."
                        ),
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
    fn flags_plural_users() {
        assert_eq!(run("`CREATE TABLE users (id INT);`").len(), 1);
    }

    #[test]
    fn flags_if_not_exists_orders() {
        assert_eq!(run("`CREATE TABLE IF NOT EXISTS orders (id INT);`").len(), 1);
    }

    #[test]
    fn allows_singular() {
        assert!(run("`CREATE TABLE user_account (id INT);`").is_empty());
    }

    #[test]
    fn allows_status_exception() {
        assert!(run("`CREATE TABLE status (id INT);`").is_empty());
    }
}
