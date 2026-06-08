//! OXC backend for elysia-cf-compile-required.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".compile()"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !ctx.project.has_framework("elysia") {
            return Vec::new();
        }
        if ctx.source_contains(".compile()") {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let AstKind::IdentifierReference(ident) = node.kind() else {
                continue;
            };
            if ident.name != "CloudflareAdapter" {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, ident.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Elysia under `CloudflareAdapter` must call `.compile()` before export."
                    .into(),
                severity: Severity::Error,
                span: None,
            });
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }


    #[test]
    fn flags_cf_without_compile() {
        let src = "import { Elysia } from 'elysia';\nimport { CloudflareAdapter } from 'elysia/adapter/cloudflare';\nexport default new Elysia({ adapter: CloudflareAdapter() });";
        assert!(!run_on(src).is_empty());
    }


    #[test]
    fn allows_cf_with_compile() {
        let src = "import { Elysia } from 'elysia';\nimport { CloudflareAdapter } from 'elysia/adapter/cloudflare';\nexport default new Elysia({ adapter: CloudflareAdapter() }).get('/', () => 'hi').compile();";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_cf_files() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().listen(3000);";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
