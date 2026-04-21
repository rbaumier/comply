use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !ctx.source.contains("accountLinking") {
            return Vec::new();
        }
        if !ctx.source.contains("enabled: true") {
            return Vec::new();
        }
        if ctx.source.contains("trustedProviders") {
            return Vec::new();
        }
        for (idx, line) in ctx.source.lines().enumerate() {
            if line.contains("accountLinking") {
                return vec![Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "better-auth-trusted-providers".into(),
                    message: "`accountLinking` is enabled without `trustedProviders` — any OAuth provider can link accounts. Add `trustedProviders` to restrict this.".into(),
                    severity: Severity::Warning,
                    span: None,
                }];
            }
        }
        Vec::new()
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
    fn flags_linking_without_trusted() {
        assert_eq!(
            run("betterAuth({ accountLinking: { enabled: true } })").len(),
            1
        );
    }
    #[test]
    fn allows_linking_with_trusted_providers() {
        assert!(run(
            "betterAuth({ accountLinking: { enabled: true, trustedProviders: ['google'] } })"
        )
        .is_empty());
    }
    #[test]
    fn ignores_non_auth_files() {
        assert!(run("const x = 42").is_empty());
    }
}
