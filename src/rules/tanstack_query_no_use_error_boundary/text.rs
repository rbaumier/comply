use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !ctx.source.contains("useQuery") {
            return vec![];
        }
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            if line.contains("useErrorBoundary") && !line.trim().starts_with("//") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: line.find("useErrorBoundary").unwrap_or(0) + 1,
                    rule_id: super::META.id.into(),
                    message: "`useErrorBoundary` was removed in v5 — use `throwOnError` instead."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), src))
    }

    #[test]
    fn flags() {
        assert_eq!(
            run("useQuery({ queryKey: ['x'], queryFn: f, useErrorBoundary: true })").len(),
            1
        );
    }

    #[test]
    fn allows() {
        assert!(
            run("useQuery({ queryKey: ['x'], queryFn: f, throwOnError: true })").is_empty()
        );
    }
}
