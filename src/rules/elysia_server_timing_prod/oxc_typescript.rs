//! elysia-server-timing-prod oxc backend — flag `serverTiming({ enabled: true })` literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, ObjectPropertyKind, PropertyKey};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["serverTiming"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let callee_name = match &call.callee {
            Expression::Identifier(id) => id.name.as_str(),
            _ => return,
        };
        if callee_name != "serverTiming" {
            return;
        }

        // Check first argument is an object with `enabled: true`.
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let Argument::ObjectExpression(obj) = first_arg else {
            return;
        };
        let has_enabled_true = obj.properties.iter().any(|prop| {
            let ObjectPropertyKind::ObjectProperty(p) = prop else {
                return false;
            };
            let key_name = match &p.key {
                PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                PropertyKey::StringLiteral(s) => s.value.as_str(),
                _ => return false,
            };
            key_name == "enabled"
                && matches!(&p.value, Expression::BooleanLiteral(b) if b.value)
        });
        if !has_enabled_true {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`serverTiming({ enabled: true })` is unconditional — gate it on an env flag."
                .into(),
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
    fn flags_enabled_true_literal() {
        let src = "import { serverTiming } from '@elysiajs/server-timing';\napp.use(serverTiming({ enabled: true }));";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_env_gated_enabled() {
        let src = "import { serverTiming } from '@elysiajs/server-timing';\napp.use(serverTiming({ enabled: process.env.NODE_ENV !== 'production' }));";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_server_timing_files() {
        let src = "serverTiming({ enabled: true });";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
