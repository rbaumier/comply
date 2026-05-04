//! OxcCheck backend — flag `throw` in modules importing better-result.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ThrowStatement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["better-result"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ThrowStatement(throw) = node.kind() else { return };
        if !ctx.source.contains("better-result") && !ctx.source.contains("@better-result") {
            return;
        }
        if inside_result_try_callback(node, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, throw.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "In modules importing better-result, throw is forbidden \u{2014} return Result.err(...) instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Walk ancestors to check if this node is inside a `Result.try(...)` or
/// `Result.tryPromise(...)` call — where throwing is the expected pattern.
fn inside_result_try_callback<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::CallExpression(call) = ancestor.kind() {
            if let Expression::StaticMemberExpression(member) = &call.callee {
                let prop = member.property.name.as_str();
                if prop == "try" || prop == "tryPromise" {
                    if let Expression::Identifier(obj) = &member.object {
                        if obj.name.as_str() == "Result" {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}
