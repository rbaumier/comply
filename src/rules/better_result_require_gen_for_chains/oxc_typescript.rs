//! OxcCheck backend for better-result-require-gen-for-chains — flag 2+ chained `.andThen()`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{CallExpression, Expression};
use std::sync::Arc;

pub struct Check;

fn is_andthen_call(call: &CallExpression) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    member.property.name.as_str() == "andThen"
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["andThen"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        if !is_andthen_call(call) {
            return;
        }

        // The callee is `obj.andThen` — check that obj is itself an andThen call.
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let Expression::CallExpression(inner_call) = &member.object else { return };

        if !is_andthen_call(inner_call) {
            return;
        }

        // Only report at the outermost andThen in a chain. If this node's
        // parent is a member expression whose parent is an andThen call, skip.
        let pid = semantic.nodes().parent_id(node.id());
        if let AstKind::StaticMemberExpression(_) = semantic.nodes().kind(pid) {
            let gpid = semantic.nodes().parent_id(pid);
            if let AstKind::CallExpression(gp_call) = semantic.nodes().kind(gpid) {
                if is_andthen_call(gp_call) {
                    return;
                }
            }
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Chaining 2+ .andThen() calls — rewrite using Result.gen + yield*.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
