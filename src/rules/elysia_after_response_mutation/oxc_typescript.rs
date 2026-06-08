//! elysia-after-response-mutation oxc backend — flag response mutation in onAfterResponse.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn has_assignment(text: &str, target: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = text[start..].find(target) {
        let after = start + pos + target.len();
        let rest = &text[after..];
        let next = rest.trim_start();
        if next.starts_with('=') && !next.starts_with("==") {
            return true;
        }
        start = after;
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
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

        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "onAfterResponse" {
            return;
        }

        // Check arguments text by looking at the full call expression source.
        let call_text =
            &ctx.source[call.span.start as usize..call.span.end as usize];
        let has_header_mutation = call_text.contains("set.headers[")
            || call_text.contains("set.headers =");
        let has_status_mutation = has_assignment(call_text, "set.status");
        let has_return = call_text.contains("return ");
        if !has_header_mutation && !has_status_mutation && !has_return {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`onAfterResponse` cannot change the response — it runs after bytes are flushed. Move mutations to `onBeforeHandle` or `mapResponse`.".into(),
            severity: Severity::Error,
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
    fn flags_set_headers_in_after() {
        let src = "import { Elysia } from 'elysia';\napp.onAfterResponse(({ set }) => {\n  set.headers['x-trace'] = 'late';\n});";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_set_status_in_after() {
        let src = "import { Elysia } from 'elysia';\napp.onAfterResponse(({ set }) => {\n  set.status = 500;\n});";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_return_in_after() {
        let src = "import { Elysia } from 'elysia';\napp.onAfterResponse(() => {\n  return { rewritten: true };\n});";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_logging_in_after() {
        let src = "import { Elysia } from 'elysia';\napp.onAfterResponse(({ request }) => {\n  console.log(request.url);\n});";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_read_only_set_status() {
        let src = "import { Elysia } from 'elysia';\napp.onAfterResponse(({ set }) => {\n  const status = typeof set.status === 'number' ? set.status : 200;\n  counter.add(1, { status });\n});";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.onAfterResponse(({ set }) => set.status = 500);";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
