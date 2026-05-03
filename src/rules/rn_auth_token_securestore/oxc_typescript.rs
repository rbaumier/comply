use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

pub struct Check;

fn key_is_sensitive(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.contains("token") || lower.contains("auth")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["AsyncStorage"])
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
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name.as_str() != "AsyncStorage" {
            return;
        }
        let prop_name = member.property.name.as_str();
        if prop_name != "setItem" && prop_name != "getItem" {
            return;
        }
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let Argument::StringLiteral(lit) = first_arg else {
            return;
        };
        let key = lit.value.as_str();
        if !key_is_sensitive(key) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "AsyncStorage is unencrypted — store `{key}` in expo-secure-store instead."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
