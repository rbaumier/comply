//! nuxt-plugin-no-sideeffect oxc backend — flag top-level side effects
//! in Nuxt plugin files outside `defineNuxtPlugin(...)`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::ast::Statement;
use std::sync::Arc;

pub struct Check;

fn is_plugin_file(ctx_path: &std::path::Path) -> bool {
    ctx_path
        .to_str()
        .map(|p| {
            p.contains("/plugins/")
                || p.contains("\\plugins\\")
                || p.starts_with("plugins/")
                || p.starts_with("plugins\\")
        })
        .unwrap_or(false)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !is_plugin_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for stmt in &semantic.nodes().program().body {
            let expr_stmt = match stmt {
                Statement::ExpressionStatement(s) => s,
                _ => continue,
            };
            let text = &ctx.source[expr_stmt.span.start as usize..expr_stmt.span.end as usize];
            let trimmed = text.trim();
            if trimmed.starts_with("defineNuxtPlugin(")
                || trimmed.starts_with("export default defineNuxtPlugin(")
            {
                continue;
            }
            if trimmed.starts_with("import ") || trimmed.starts_with("export ") {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, expr_stmt.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Top-level side effect in a Nuxt plugin — move it inside `defineNuxtPlugin((nuxtApp) => { ... })`.".into(),
                severity: Severity::Error,
                span: Some((expr_stmt.span.start as usize, (expr_stmt.span.end - expr_stmt.span.start) as usize)),
            });
        }
        diagnostics
    }
}
