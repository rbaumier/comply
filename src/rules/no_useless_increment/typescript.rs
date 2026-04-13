//! no-useless-increment AST backend — `return x++` returns value before mutation.

use crate::diagnostic::{Diagnostic, Severity};

/// Matches `return <identifier>++;` or `return <identifier>--;`.
fn has_useless_post_increment(line: &str) -> bool {
    let trimmed = line.trim();
    let rest = match trimmed.strip_prefix("return ") {
        Some(r) => r,
        None => return false,
    };
    let rest = rest.trim_start();
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

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");
    for (idx, line) in text.lines().enumerate() {
        if has_useless_post_increment(line) {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "no-useless-increment".into(),
                message: "`return x++` / `return x--` returns the value before the mutation — use prefix or separate statements.".into(),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_return_post_increment() {
        assert_eq!(run_on("return x++;").len(), 1);
    }

    #[test]
    fn flags_return_post_decrement() {
        assert_eq!(run_on("return count--;").len(), 1);
    }

    #[test]
    fn allows_prefix_increment() {
        assert!(run_on("return ++x;").is_empty());
    }

    #[test]
    fn allows_plain_return() {
        assert!(run_on("return x;").is_empty());
    }
}
