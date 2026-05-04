//! tanstack-query-no-cache-time OXC backend — flag `cacheTime` property in
//! TanStack Query calls (renamed to `gcTime` in v5).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, PropertyKey};
use oxc_span::GetSpan;
use std::sync::Arc;

const SCOPES: &[&str] = &[
    "QueryClient",
    "useQuery",
    "useSuspenseQuery",
    "useInfiniteQuery",
    "useSuspenseInfiniteQuery",
    "useQueries",
    "queryOptions",
];

pub struct Check;

fn inside_known_scope<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node.id();
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            break;
        }
        let parent = nodes.get_node(parent_id);
        match parent.kind() {
            AstKind::CallExpression(call) => {
                let name = match &call.callee {
                    Expression::Identifier(id) => Some(id.name.as_str()),
                    Expression::StaticMemberExpression(m) => Some(m.property.name.as_str()),
                    _ => None,
                };
                if let Some(n) = name {
                    if SCOPES.contains(&n) {
                        return true;
                    }
                }
            }
            AstKind::NewExpression(new_expr) => {
                let name = match &new_expr.callee {
                    Expression::Identifier(id) => Some(id.name.as_str()),
                    _ => None,
                };
                if let Some(n) = name {
                    if SCOPES.contains(&n) {
                        return true;
                    }
                }
            }
            _ => {}
        }
        current_id = parent_id;
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["cacheTime"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectProperty(prop) = node.kind() else {
            return;
        };
        let key_name = match &prop.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => return,
        };
        if key_name != "cacheTime" {
            return;
        }
        if !inside_known_scope(node, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, prop.key.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`cacheTime` was renamed to `gcTime` in TanStack Query v5.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
