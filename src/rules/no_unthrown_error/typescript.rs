//! no-unthrown-error AST backend — `new Error(...)` created but never thrown.

use crate::diagnostic::{Diagnostic, Severity};

fn is_unthrown_error(line: &str) -> bool {
    let trimmed = line.trim();

    if !trimmed.contains("new Error(") {
        return false;
    }

    // Skip comments.
    if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
        return false;
    }

    // If thrown, returned, or assigned, it's fine.
    if trimmed.starts_with("throw ")
        || trimmed.starts_with("throw(")
        || trimmed.starts_with("return ")
        || trimmed.starts_with("return(")
        || trimmed.contains('=')
        || trimmed.starts_with("yield ")
        || trimmed.starts_with("const ")
        || trimmed.starts_with("let ")
        || trimmed.starts_with("var ")
        || trimmed.starts_with("export ")
    {
        return false;
    }

    true
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");
    for (idx, line) in text.lines().enumerate() {
        if is_unthrown_error(line) {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "no-unthrown-error".into(),
                message: "`new Error(...)` is created but never thrown — add `throw` or assign the error.".into(),
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
    fn flags_unthrown_error() {
        assert_eq!(run_on("  new Error(\"oops\");").len(), 1);
    }

    #[test]
    fn flags_bare_new_error() {
        assert_eq!(run_on("new Error(\"something went wrong\");").len(), 1);
    }

    #[test]
    fn allows_thrown_error() {
        assert!(run_on("throw new Error(\"oops\");").is_empty());
    }

    #[test]
    fn allows_assigned_error() {
        assert!(run_on("const err = new Error(\"oops\");").is_empty());
    }

    #[test]
    fn allows_returned_error() {
        assert!(run_on("return new Error(\"oops\");").is_empty());
    }
}
