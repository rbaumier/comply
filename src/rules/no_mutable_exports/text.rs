use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            let kind = if trimmed.starts_with("export let ") {
                Some("let")
            } else if trimmed.starts_with("export var ") {
                Some("var")
            } else {
                None
            };
            if let Some(k) = kind {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-mutable-exports".into(),
                    message: format!(
                        "Exporting mutable `{}` binding — use `export const` instead.",
                        k
                    ),
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
    fn flags_export_let() {
        let src = "export let count = 0;\n";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`let`"));
    }

    #[test]
    fn flags_export_var() {
        let src = "export var name = 'x';\n";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`var`"));
    }

    #[test]
    fn allows_export_const() {
        let src = "export const MAX = 10;\n";
        assert!(run(src).is_empty());
    }
}
