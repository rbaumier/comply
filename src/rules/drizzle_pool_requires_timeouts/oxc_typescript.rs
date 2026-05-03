//! drizzle-pool-requires-timeouts oxc backend — flag `new Pool({...})` where
//! the object literal doesn't contain both `idleTimeoutMillis` and
//! `connectionTimeoutMillis` keys.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind, PropertyKey};
use std::sync::Arc;

pub struct Check;

fn has_key(props: &oxc_allocator::Vec<'_, ObjectPropertyKind<'_>>, key: &str) -> bool {
    props.iter().any(|prop| {
        let ObjectPropertyKind::ObjectProperty(p) = prop else { return false };
        match &p.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str() == key,
            PropertyKey::StringLiteral(s) => s.value.as_str() == key,
            _ => false,
        }
    })
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Pool"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NewExpression(new_expr) = node.kind() else { return };

        let Expression::Identifier(ctor) = &new_expr.callee else { return };
        if ctor.name.as_str() != "Pool" {
            return;
        }

        // Find the first object argument.
        let obj = new_expr.arguments.iter().find_map(|arg| {
            match arg {
                oxc_ast::ast::Argument::ObjectExpression(obj) => Some(obj),
                _ => None,
            }
        });

        let Some(obj) = obj else {
            // No config object at all.
            let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`new Pool()` must pass a config object with `idleTimeoutMillis` and `connectionTimeoutMillis`.".into(),
                severity: Severity::Warning,
                span: None,
            });
            return;
        };

        let has_idle = has_key(&obj.properties, "idleTimeoutMillis");
        let has_conn = has_key(&obj.properties, "connectionTimeoutMillis");
        if has_idle && has_conn {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`new Pool()` must set both `idleTimeoutMillis` and `connectionTimeoutMillis` so stuck connections don't leak and new ones fail fast.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
