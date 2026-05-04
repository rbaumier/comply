//! no-unchecked-json-parse OXC backend — flag unwrapped `JSON.parse(...)` calls.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["JSON"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Check if callee is `JSON.parse`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "parse" {
            return;
        }
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name.as_str() != "JSON" {
            return;
        }

        // Check if wrapped in a validator: parent is an argument to .parse()/.safeParse().
        let parent = semantic.nodes().parent_node(node.id());
        if let AstKind::CallExpression(outer_call) = parent.kind()
            && let Expression::StaticMemberExpression(outer_member) = &outer_call.callee {
                let method = outer_member.property.name.as_str();
                if method == "parse" || method == "safeParse" {
                    return;
                }
            }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`JSON.parse()` returns `any` — wrap it with a Zod schema or type guard before using the result.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
