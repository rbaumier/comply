//! drizzle-json-requires-type OXC backend — flag `json()`/`jsonb()` without
//! `.$type<T>()` in the call chain.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// Walk upward through chained `.foo()` calls and check if any in the chain
/// is `.$type(...)`.
fn chain_has_type_call<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node.id();
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        match parent.kind() {
            AstKind::StaticMemberExpression(member) => {
                if member.property.name == "$type" {
                    return true;
                }
                current_id = parent_id;
            }
            AstKind::ComputedMemberExpression(_) => {
                current_id = parent_id;
            }
            AstKind::CallExpression(_) => {
                current_id = parent_id;
            }
            _ => return false,
        }
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["$type"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let name = match &call.callee {
            Expression::Identifier(id) => id.name.as_str(),
            _ => return,
        };
        if name != "json" && name != "jsonb" {
            return;
        }

        if chain_has_type_call(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`json()`/`jsonb()` without `.$type<T>()` \u{2014} the column will infer as `unknown`. Chain `.$type<T>()` to preserve the payload shape.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
