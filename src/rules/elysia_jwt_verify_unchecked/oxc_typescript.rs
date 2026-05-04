//! elysia-jwt-verify-unchecked oxc backend — flag unchecked jwt.verify results.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["jwt"])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !ctx.project.has_framework("elysia") || !ctx.source.contains("jwt") {
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
                    path: Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "`jwt.verify(...)` result is used without a falsy check — invalid tokens are not rejected.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
    }
}
