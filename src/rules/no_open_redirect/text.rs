use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const REDIRECT_FNS: &[&str] = &[
    "res.redirect(",
    "response.redirect(",
    "redirect(",
    "router.redirect(",
];
const USER_DATA: &[&str] = &[
    "req.query.",
    "req.params.",
    "req.body.",
    "request.query.",
    "searchParams.get(",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if !REDIRECT_FNS.iter().any(|f| t.contains(f)) {
                continue;
            }
            if USER_DATA.iter().any(|u| t.contains(u)) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-open-redirect".into(),
                    message: "Redirect target from user input — validate against an allowlist before redirecting.".into(),
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
    fn flags_redirect_with_query_param() {
        assert_eq!(run("res.redirect(req.query.returnUrl)").len(), 1);
    }
    #[test]
    fn flags_redirect_with_search_params() {
        assert_eq!(run("redirect(searchParams.get('next'))").len(), 1);
    }
    #[test]
    fn allows_literal_redirect() {
        assert!(run("res.redirect('/dashboard')").is_empty());
    }
    #[test]
    fn allows_validated_redirect() {
        assert!(run("res.redirect(safeUrl)").is_empty());
    }
}
