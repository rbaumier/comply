//! elysia-jwt-verify-unchecked backend — flag unchecked jwt.verify results.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["jwt"])
    }

    fn check(&self, ctx: &CheckCtx, _tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        if !ctx.project.has_framework("elysia") || !ctx.source_contains("jwt") {
            return Vec::new();
        }

        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut diagnostics = Vec::new();

        for (idx, line) in lines.iter().enumerate() {
            if !line.contains("jwt.verify(") {
                continue;
            }
            // Need the result to be assigned (`= await jwt.verify`).
            let norm: String = line.chars().filter(|c| !c.is_whitespace()).collect();
            if !norm.contains("=awaitjwt.verify(") && !norm.contains("=jwt.verify(") {
                continue;
            }

            // Look ahead 3 lines for `if (!`.
            let end = (idx + 4).min(lines.len());
            let mut checked = false;
            for next in &lines[idx + 1..end] {
                let nn: String = next.chars().filter(|c| !c.is_whitespace()).collect();
                if nn.contains("if(!") || nn.contains("?") || nn.contains("throw") {
                    checked = true;
                    break;
                }
            }

            if !checked {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: "elysia-jwt-verify-unchecked".into(),
                    message: "`jwt.verify(...)` result is used without a falsy check — invalid tokens are not rejected.".into(),
                    severity: Severity::Warning,
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_unchecked_verify() {
        let src = "import { jwt } from 'elysia';\nconst payload = await jwt.verify(token);\nreturn payload.userId;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_checked_verify() {
        let src = "import { jwt } from 'elysia';\nconst payload = await jwt.verify(token);\nif (!payload) return status(401);\nreturn payload.userId;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "const payload = await jwt.verify(token);\nreturn payload.userId;";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
