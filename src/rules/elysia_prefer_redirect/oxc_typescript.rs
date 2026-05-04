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
        let s = ctx.source;
        let has_location = s.contains("set.headers.location")
            || s.contains("set.headers['location']")
            || s.contains("set.headers[\"location\"]")
            || s.contains("set.headers.Location")
            || s.contains("set.headers['Location']")
            || s.contains("set.headers[\"Location\"]");
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
