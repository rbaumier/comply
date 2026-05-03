use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !ctx.project.has_framework("elysia") {
            return Vec::new();
        }

        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut diagnostics = Vec::new();

        for (idx, line) in lines.iter().enumerate() {
            if !line.contains(".set({") {
                continue;
            }
            let end = (idx + 6).min(lines.len());
            let block: String = lines[idx..end].join("\n");
            let norm: String = block.chars().filter(|c| !c.is_whitespace()).collect();
            if norm.contains("httpOnly:true") {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line: idx + 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: "Cookie `.set({...})` without `httpOnly: true` — JWT is readable from JavaScript (XSS).".into(),
                severity: Severity::Error,
                span: None,
            });
        }
        diagnostics
    }
}
