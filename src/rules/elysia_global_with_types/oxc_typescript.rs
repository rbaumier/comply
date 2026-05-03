//! elysia-global-with-types OXC backend — flag global-scoped plugins that expose typed context.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

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

        // Cheap textual gate: must contain a global scope marker AND a typed-state method.
        let s = ctx.source;
        let has_global = s.contains("as:'global'")
            || s.contains("as: 'global'")
            || s.contains("as:\"global\"")
            || s.contains("as: \"global\"")
            || s.contains(".as('global')")
            || s.contains(".as(\"global\")");
        if !has_global {
            return;
        }
        let has_typed = s.contains(".state(") || s.contains(".decorate(") || s.contains(".model(");
        if !has_typed {
            return;
        }

        // Only emit once — anchor on the first `.state(`, `.decorate(`, or `.model(` call.
        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let prop = member.property.name.as_str();
        if prop != "state" && prop != "decorate" && prop != "model" {
            return;
        }

        // Avoid duplicates: only flag if no diagnostic for this rule has been pushed yet.
        if diagnostics
            .iter()
            .any(|d| d.rule_id == "elysia-global-with-types")
        {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Global-scoped plugin exposes typed context (`state`/`decorate`/`model`) — types leak into every consumer. Use `as: 'scoped'`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
