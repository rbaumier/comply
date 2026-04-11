use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `public static` or `static public` fields without `readonly`.
fn missing_readonly(line: &str) -> bool {
    let trimmed = line.trim();
    let has_public_static =
        trimmed.contains("public static") || trimmed.contains("static public");
    if !has_public_static {
        return false;
    }
    // Must have `=` to be a field (not a method)
    if !trimmed.contains('=') {
        return false;
    }
    // If it already has readonly, it's fine
    if trimmed.contains("readonly") {
        return false;
    }
    true
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if missing_readonly(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "public-static-readonly".into(),
                    message:
                        "`public static` field is missing `readonly` — add it to prevent mutation."
                            .into(),
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
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_public_static_without_readonly() {
        assert_eq!(run("  public static MAX = 100;").len(), 1);
    }

    #[test]
    fn flags_static_public_without_readonly() {
        assert_eq!(run("  static public MAX = 100;").len(), 1);
    }

    #[test]
    fn allows_public_static_readonly() {
        assert!(run("  public static readonly MAX = 100;").is_empty());
    }

    #[test]
    fn allows_public_static_method() {
        assert!(run("  public static getInstance() {").is_empty());
    }
}
