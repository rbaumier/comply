//! next-no-api-route-in-middleware OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_middleware_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy().replace('\\', "/");
    s.ends_with("/middleware.ts")
        || s.ends_with("/middleware.tsx")
        || s.ends_with("/middleware.js")
        || s == "middleware.ts"
        || s == "middleware.tsx"
        || s == "middleware.js"
}

fn looks_like_internal_api_path(text: &str) -> bool {
    let trimmed = text.trim().trim_matches(|c| c == '"' || c == '\'' || c == '`');
    trimmed.starts_with("/api/") || trimmed == "/api"
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["fetch"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.project.has_framework("nextjs") {
            return;
        }
        if !is_middleware_file(ctx.path) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be `fetch`
        let Expression::Identifier(id) = &call.callee else { return };
        if id.name.as_str() != "fetch" {
            return;
        }

        let Some(first) = call.arguments.first() else { return };
        let span = first.span();
        let text = &ctx.source[span.start as usize..span.end as usize];
        if !looks_like_internal_api_path(text) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Don't fetch `/api/*` from middleware \u{2014} it triggers a same-origin loop. Inline the logic instead.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ProjectCtx;
    use crate::rules::file_ctx::FileCtx;



    fn next_project() -> ProjectCtx {
        let mut project = ProjectCtx::empty();
        project.framework = Framework::NextJs;
        project
    }


    fn run(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx_with_project(
            source,
            &Check,
            &next_project())
    }


    #[test]
    fn flags_fetch_api_in_middleware() {
        let src = "export async function middleware() { await fetch('/api/auth'); }";
        assert_eq!(run(src, "middleware.ts").len(), 1);
    }


    #[test]
    fn allows_fetch_external_in_middleware() {
        let src = "export async function middleware() { await fetch('https://example.com/x'); }";
        assert!(run(src, "middleware.ts").is_empty());
    }


    #[test]
    fn allows_fetch_api_outside_middleware() {
        let src = "async function load() { await fetch('/api/auth'); }";
        assert!(run(src, "src/lib/load.ts").is_empty());
    }


    #[test]
    fn flags_src_middleware_file() {
        let src = "export async function middleware() { await fetch('/api/x'); }";
        assert_eq!(run(src, "src/middleware.ts").len(), 1);
    }
}
