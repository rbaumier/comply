use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__") || s.contains("_test.")
}

/// Detect `.toThrow()` with empty parens (possibly whitespace inside).
fn has_empty_to_throw(line: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = line[start..].find(".toThrow(") {
        let abs = start + pos + 9; // skip past ".toThrow("
        let rest = &line[abs..];
        let trimmed = rest.trim_start();
        if trimmed.starts_with(')') {
            return true;
        }
        start = abs;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_test_file(ctx.path) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_empty_to_throw(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "test-check-exception".into(),
                    message: "`.toThrow()` without specifying error type or message — any error will pass.".into(),
                    severity: Severity::Warning,
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
        Check.check(&CheckCtx::for_test(Path::new("foo.test.ts"), source))
    }

    #[test]
    fn flags_empty_to_throw() {
        assert_eq!(run("  expect(() => doThing()).toThrow();").len(), 1);
    }

    #[test]
    fn flags_to_throw_with_whitespace() {
        assert_eq!(run("  expect(() => doThing()).toThrow(  );").len(), 1);
    }

    #[test]
    fn allows_to_throw_with_error_type() {
        assert!(run("  expect(() => doThing()).toThrow(TypeError);").is_empty());
    }

    #[test]
    fn allows_to_throw_with_message() {
        assert!(run(r#"  expect(() => doThing()).toThrow("bad input");"#).is_empty());
    }

    #[test]
    fn ignores_non_test_files() {
        let diags = Check.check(&CheckCtx::for_test(
            Path::new("foo.ts"),
            "  expect(() => doThing()).toThrow();",
        ));
        assert!(diags.is_empty());
    }
}
