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
        let has_global = ctx.source_contains("as:'global'")
            || ctx.source_contains("as: 'global'")
            || ctx.source_contains("as:\"global\"")
            || ctx.source_contains("as: \"global\"")
            || ctx.source_contains(".as('global')")
            || ctx.source_contains(".as(\"global\")");
        if !has_global {
            return;
        }
        let has_typed = ctx.source_contains(".state(") || ctx.source_contains(".decorate(") || ctx.source_contains(".model(");
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

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }


    #[test]
    fn flags_global_with_state() {
        let src = "import { Elysia } from 'elysia';\nexport const p = new Elysia().state('x', 1).onBeforeHandle({ as: 'global' }, () => {});";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_global_with_decorate() {
        let src = "import { Elysia } from 'elysia';\nexport const p = new Elysia().decorate('foo', 1).as('global');";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_scoped_with_state() {
        let src = "import { Elysia } from 'elysia';\nexport const p = new Elysia().state('x', 1).as('scoped');";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_global_without_typed_state() {
        let src = "import { Elysia } from 'elysia';\nexport const p = new Elysia().onBeforeHandle({ as: 'global' }, () => {});";
        assert!(run_on(src).is_empty());
    }
}
