use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["hono/cors"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.source_contains("hono/cors") {
            return;
        }
        let AstKind::CallExpression(call) = node.kind() else { return };
        let oxc_ast::ast::Expression::Identifier(callee) = &call.callee else {
            return;
        };
        if callee.name != "cors" {
            return;
        }

        let span_start = call.span.start as usize;

        // `cors()` with no arguments.
        if call.arguments.is_empty() {
            let (line, column) = byte_offset_to_line_col(ctx.source, span_start);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`cors()` without arguments defaults to `origin: '*'` — any origin can access the API.".into(),
                severity: Severity::Error,
                span: None,
            });
            return;
        }

        // Check the first argument (the options object).
        let args_text = &ctx.source[call.span.start as usize..call.span.end as usize];
        let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();

        // `origin: '*'` or `origin: "*"`.
        if norm.contains("origin:'*'") || norm.contains("origin:\"*\"") {
            let (line, column) = byte_offset_to_line_col(ctx.source, span_start);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`origin: '*'` allows any origin to access the API.".into(),
                severity: Severity::Error,
                span: None,
            });
        }

        // `credentials: true` without a specific origin.
        if norm.contains("credentials:true") {
            let has_specific_origin = norm.contains("origin:")
                && !norm.contains("origin:'*'")
                && !norm.contains("origin:\"*\"");
            if !has_specific_origin {
                let (line, column) = byte_offset_to_line_col(ctx.source, span_start);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`credentials: true` without a specific origin — any origin can make credentialed requests.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_bare_cors() {
        let src = "import { cors } from 'hono/cors';\napp.use(cors());";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_wildcard_origin() {
        let src = "import { cors } from 'hono/cors';\napp.use(cors({ origin: '*' }));";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_credentials_without_origin() {
        let src = "import { cors } from 'hono/cors';\napp.use(cors({\n  credentials: true\n}));";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_specific_origin() {
        let src =
            "import { cors } from 'hono/cors';\napp.use(cors({ origin: 'https://example.com' }));";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_credentials_with_specific_origin() {
        let src = "import { cors } from 'hono/cors';\napp.use(cors({\n  origin: 'https://example.com',\n  credentials: true\n}));";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_hono_files() {
        let src = "app.use(cors());";
        assert!(run_on(src).is_empty());
    }
}
