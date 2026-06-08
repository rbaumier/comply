//! elysia-prefer-redirect OXC backend — flag manual redirect patterns.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AssignmentExpression]
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

        let AstKind::AssignmentExpression(assign) = node.kind() else { return };

        let left_span = assign.left.span();
        let left_text = &ctx.source[left_span.start as usize..left_span.end as usize];
        if left_text != "set.status" {
            return;
        }

        let right_span = assign.right.span();
        let right_text = ctx.source[right_span.start as usize..right_span.end as usize].trim();
        if right_text != "301" && right_text != "302" && right_text != "303" && right_text != "307" && right_text != "308" {
            return;
        }

        // Confirm the file actually sets a Location header somewhere.
        let has_location = ctx.source_contains("set.headers.location")
            || ctx.source_contains("set.headers['location']")
            || ctx.source_contains("set.headers[\"location\"]")
            || ctx.source_contains("set.headers.Location")
            || ctx.source_contains("set.headers['Location']")
            || ctx.source_contains("set.headers[\"Location\"]");
        if !has_location {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, assign.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Manual redirect via `set.status` + `set.headers.location` — return `redirect(url, code)` instead.".into(),
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
    fn flags_manual_302_redirect() {
        let src = "import { Elysia } from 'elysia';\napp.get('/', ({ set }) => { set.status = 302; set.headers.location = '/new'; });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_manual_301_redirect() {
        let src = "import { Elysia } from 'elysia';\napp.get('/', ({ set }) => { set.status = 301; set.headers['Location'] = '/new'; });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_redirect_helper() {
        let src = "import { Elysia } from 'elysia';\napp.get('/', ({ redirect }) => redirect('/new', 302));";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_redirect_status() {
        let src =
            "import { Elysia } from 'elysia';\napp.get('/', ({ set }) => { set.status = 401; });";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
