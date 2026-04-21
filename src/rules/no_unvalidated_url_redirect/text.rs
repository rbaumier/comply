use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const LOCATION_PATTERNS: &[&str] = &[
    "window.location.href =",
    "location.href =",
    "location.replace(",
    "location.assign(",
];
const USER_DATA: &[&str] = &["searchParams.get(", "req.", "params.", "query."];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if !LOCATION_PATTERNS.iter().any(|p| t.contains(p)) {
                continue;
            }
            if USER_DATA.iter().any(|u| t.contains(u)) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-unvalidated-url-redirect".into(),
                    message: "Client-side redirect target from user input — validate the URL before redirecting.".into(),
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
    fn flags_location_href_from_search_params() {
        assert_eq!(
            run("window.location.href = searchParams.get('next')").len(),
            1
        );
    }
    #[test]
    fn flags_location_replace_with_query() {
        assert_eq!(run("location.replace(query.redirectUrl)").len(), 1);
    }
    #[test]
    fn allows_literal_location() {
        assert!(run("window.location.href = '/dashboard'").is_empty());
    }
    #[test]
    fn allows_validated_var() {
        assert!(run("window.location.href = safeUrl").is_empty());
    }
}
