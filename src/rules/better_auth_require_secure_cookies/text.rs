use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !ctx.source.contains("betterAuth(") {
            return Vec::new();
        }
        if ctx.source.contains("useSecureCookies:") {
            return Vec::new();
        }
        for (idx, line) in ctx.source.lines().enumerate() {
            if line.contains("betterAuth(") {
                return vec![Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "better-auth-require-secure-cookies".into(),
                    message: "Better Auth config is missing `useSecureCookies: true` — add `advanced: { useSecureCookies: true }` so session cookies are only sent over HTTPS.".into(),
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
        Check.check(&CheckCtx::for_test(Path::new("auth.ts"), s))
    }
    #[test]
    fn flags_missing_secure_cookies() {
        assert_eq!(
            run("export const auth = betterAuth({ database: db });").len(),
            1
        );
    }
    #[test]
    fn allows_with_secure_cookies() {
        assert!(
            run("betterAuth({ advanced: { useSecureCookies: true }, database: db })").is_empty()
        );
    }
    #[test]
    fn ignores_file_without_better_auth() {
        assert!(run("const x = doSomething()").is_empty());
    }
}
