use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if line.trim().contains("disableOriginCheck: true") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "better-auth-no-disable-origin-check".into(),
                    message: "`disableOriginCheck: true` removes origin validation — remove this option.".into(),
                    severity: Severity::Error,
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
    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), s))
    }
    #[test]
    fn flags_disable_origin() {
        assert_eq!(run("betterAuth({ disableOriginCheck: true })").len(), 1);
    }
    #[test]
    fn allows_trusted_origins() {
        assert!(run("betterAuth({ trustedOrigins: ['https://app.example.com'] })").is_empty());
    }
}
