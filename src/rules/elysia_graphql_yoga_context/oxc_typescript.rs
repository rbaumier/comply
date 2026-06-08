//! elysia-graphql-yoga-context oxc backend — flag `yoga({ context })` without `useContext`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan as _;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useContext"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };

        let callee_text = &ctx.source[call.callee.span().start as usize..call.callee.span().end as usize];
        if callee_text != "yoga" {
            return;
        }

        let call_text = &ctx.source[call.span.start as usize..call.span.end as usize];
        let norm: String = call_text.chars().filter(|c| !c.is_whitespace()).collect();

        if !norm.contains("context:") {
            return;
        }
        if ctx.source_contains("useContext") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`yoga({ context })` without a `useContext` placeholder — resolvers will not see the context.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }


    #[test]
    fn flags_context_without_placeholder() {
        let src = "import { yoga } from '@elysiajs/graphql-yoga';\napp.use(yoga({ context: () => ({ user: 1 }) }));";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_when_use_context_present() {
        let src = "import { yoga } from '@elysiajs/graphql-yoga';\nimport { useContext } from './ctx';\napp.use(yoga({ context: () => ({ user: 1 }) }));";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_when_no_context_field() {
        let src = "import { yoga } from '@elysiajs/graphql-yoga';\napp.use(yoga({ typeDefs }));";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_yoga_files() {
        let src = "yoga({ context: () => ({}) });";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
