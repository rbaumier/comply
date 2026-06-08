use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn inside_on_error<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::CallExpression(call) = ancestor.kind() {
            let callee_text =
                &source[call.callee.span().start as usize..call.callee.span().end as usize];
            if callee_text.ends_with(".onError") || callee_text == "onError" {
                return true;
            }
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StaticMemberExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["hono", "Hono"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.source_contains("hono") && !ctx.source_contains("Hono") {
            return;
        }

        let AstKind::StaticMemberExpression(member) = node.kind() else {
            return;
        };

        let prop_name = member.property.name.as_str();
        if prop_name != "stack" && prop_name != "message" {
            return;
        }

        // Object should be a simple identifier like err/error/e/exception
        let Expression::Identifier(obj_id) = &member.object else {
            return;
        };
        let obj_name = obj_id.name.as_str();
        if !matches!(obj_name, "err" | "error" | "e" | "exception") {
            return;
        }

        if !inside_on_error(node, semantic, ctx.source) {
            return;
        }

        let member_text =
            &ctx.source[member.span.start as usize..member.span.end as usize];
        let (line, column) =
            byte_offset_to_line_col(ctx.source, member.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Returning `{member_text}` from `onError` leaks internal error details to clients."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}
