use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `?:` combined with `| undefined` on the same line.
fn has_redundant_optional(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.contains("?:") && trimmed.contains("| undefined")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_redundant_optional(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-redundant-optional".into(),
                    message:
                        "`?:` already implies `| undefined` — remove the redundant union member."
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
    fn flags_optional_with_undefined() {
        assert_eq!(run("  name?: string | undefined;").len(), 1);
    }

    #[test]
    fn flags_optional_with_undefined_complex() {
        assert_eq!(run("  value?: number | null | undefined;").len(), 1);
    }

    #[test]
    fn allows_optional_without_undefined() {
        assert!(run("  name?: string;").is_empty());
    }

    #[test]
    fn allows_required_with_undefined() {
        assert!(run("  name: string | undefined;").is_empty());
    }
}
