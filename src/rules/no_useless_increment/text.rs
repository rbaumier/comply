use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Matches `return <identifier>++;` or `return <identifier>--;`.
fn has_useless_post_increment(line: &str) -> bool {
    let trimmed = line.trim();
    let rest = match trimmed.strip_prefix("return ") {
        Some(r) => r,
        None => return false,
    };
    // After "return " we expect an identifier then ++ or -- then optional ;
    let rest = rest.trim_start();
    // Find ++ or --
    if let Some(pos) = rest.find("++") {
        let ident = rest[..pos].trim();
        let after = rest[pos + 2..].trim().trim_end_matches(';').trim();
        if !ident.is_empty() && ident.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$' || c == '.') && after.is_empty() {
            return true;
        }
    }
    if let Some(pos) = rest.find("--") {
        let ident = rest[..pos].trim();
        let after = rest[pos + 2..].trim().trim_end_matches(';').trim();
        if !ident.is_empty() && ident.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$' || c == '.') && after.is_empty() {
            return true;
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_useless_post_increment(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-useless-increment".into(),
                    message: "`return x++` / `return x--` returns the value before the mutation — use prefix or separate statements.".into(),
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
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_return_post_increment() {
        assert_eq!(run("return x++;").len(), 1);
    }

    #[test]
    fn flags_return_post_decrement() {
        assert_eq!(run("return count--;").len(), 1);
    }

    #[test]
    fn allows_prefix_increment() {
        assert!(run("return ++x;").is_empty());
    }

    #[test]
    fn allows_plain_return() {
        assert!(run("return x;").is_empty());
    }
}
