use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const ROUTE_INDICATORS: &[&str] = &[
    ".get(",
    ".post(",
    ".put(",
    ".delete(",
    ".patch(",
    "from 'hono'",
    "from \"hono\"",
    "@hono/",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // Check if this file is a route handler file.
        let is_route_file = ctx
            .source
            .lines()
            .any(|line| ROUTE_INDICATORS.iter().any(|p| line.contains(p)));

        if !is_route_file {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.contains("new Error(") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "structured-api-error".into(),
                    message: "Bare `new Error()` in route handler — use a structured error with `{ type, code, status, detail }`.".into(),
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
    fn flags_bare_error_in_route_file() {
        let src = r#"
import { Hono } from "hono";
app.get("/foo", (c) => {
    throw new Error("not found");
});
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_error_in_non_route_file() {
        let src = r#"
function validate(x: string) {
    throw new Error("invalid input");
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_multiple_errors() {
        let src = r#"
app.post("/bar", (c) => {
    if (!x) throw new Error("missing x");
    if (!y) throw new Error("missing y");
});
"#;
        assert_eq!(run(src).len(), 2);
    }
}
