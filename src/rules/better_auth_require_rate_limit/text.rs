use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let has_auth = ctx.source.contains("betterAuth(") || ctx.source.contains("createAuth(");
        if !has_auth {
            return Vec::new();
        }
        if ctx.source.contains("rateLimit") {
            return Vec::new();
        }
        for (idx, line) in ctx.source.lines().enumerate() {
            if line.contains("betterAuth(") || line.contains("createAuth(") {
                return vec![Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "better-auth-require-rate-limit".into(),
                    message: "Better Auth config is missing `rateLimit` — add `rateLimit: { enabled: true }` to protect auth endpoints.".into(),
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
    fn flags_missing_rate_limit() {
        assert_eq!(
            run("export const auth = betterAuth({ database: db })").len(),
            1
        );
    }
    #[test]
    fn allows_with_rate_limit() {
        assert!(
            run("export const auth = betterAuth({ rateLimit: { enabled: true } })").is_empty()
        );
    }
    #[test]
    fn ignores_non_auth_files() {
        assert!(run("const x = doSomething()").is_empty());
    }
}
