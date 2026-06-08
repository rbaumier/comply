use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

fn is_nuxt_source(src: &str) -> bool {
    src.contains("#imports")
        || src.contains("nuxt/app")
        || src.contains("#app")
        || src.contains("defineNuxtConfig")
        || src.contains("defineNuxtPlugin")
        || src.contains("defineNuxtRouteMiddleware")
        || src.contains("useRuntimeConfig")
        || src.contains("useNuxtApp")
}

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["process"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::MemberExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_nuxt_source(ctx.source) {
            return;
        }

        let AstKind::MemberExpression(member) = node.kind() else { return };

        let full_span = member.span();
        let full_text = &ctx.source[full_span.start as usize..full_span.end as usize];
        if full_text != "process.env" && !full_text.starts_with("process.env.") {
            return;
        }

        let obj = member.object();
        let is_process = matches!(obj, Expression::Identifier(id) if id.name == "process");
        if !is_process {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, full_span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`process.env` is unavailable on the client; use `useRuntimeConfig()` instead.".into(),
            severity: Severity::Error,
            span: Some((full_span.start as usize, full_span.size() as usize)),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_process_env_access() {
        let src = "import {} from '#imports';\nconst k = process.env.API_KEY;";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_use_runtime_config() {
        let src = "import {} from '#imports';\nconst cfg = useRuntimeConfig();\nconst k = cfg.public.apiBase;";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_nuxt_files() {
        let src = "const k = process.env.API_KEY;";
        assert!(run_on(src).is_empty());
    }
}
