//! tanstack-query-no-enabled-true OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

const HOOKS: &[&str] = &[
    "useQuery",
    "useSuspenseQuery",
    "useInfiniteQuery",
    "useSuspenseInfiniteQuery",
    "useQueries",
    "queryOptions",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
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

        // Check key is "enabled"
        let key_name = match &prop.key {
            oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            oxc_ast::ast::PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => return,
        };
        if key_name != "enabled" {
            return;
        }

        // Check value is `true`
        let Expression::BooleanLiteral(lit) = &prop.value else {
            return;
        };
        if !lit.value {
            return;
        }

        // Walk parents to find a query hook call
        if !inside_query_hook(node, semantic, ctx.source) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, prop.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`enabled: true` is redundant — queries are enabled by default.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn inside_query_hook<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> bool {
    for ancestor_id in semantic.nodes().ancestor_ids(node.id()) {
        let ancestor = semantic.nodes().get_node(ancestor_id);
        if let AstKind::CallExpression(call) = ancestor.kind() {
            let callee_start = call.callee.span().start as usize;
            let callee_end = call.callee.span().end as usize;
            let callee_text = &source[callee_start..callee_end.min(source.len())];
            if HOOKS.contains(&callee_text) {
                return true;
            }
        }
    }
    false
}
