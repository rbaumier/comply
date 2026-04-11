use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_unthrown_error(line: &str) -> bool {
    let trimmed = line.trim();

    // Must contain `new Error(`.
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

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if is_unthrown_error(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-unthrown-error".into(),
                    message: "`new Error(...)` is created but never thrown — add `throw` or assign the error.".into(),
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
    fn flags_unthrown_error() {
        assert_eq!(run("  new Error(\"oops\");").len(), 1);
    }

    #[test]
    fn flags_bare_new_error() {
        assert_eq!(run("new Error(\"something went wrong\");").len(), 1);
    }

    #[test]
    fn allows_thrown_error() {
        assert!(run("throw new Error(\"oops\");").is_empty());
    }

    #[test]
    fn allows_assigned_error() {
        assert!(run("const err = new Error(\"oops\");").is_empty());
    }

    #[test]
    fn allows_returned_error() {
        assert!(run("return new Error(\"oops\");").is_empty());
    }
}
