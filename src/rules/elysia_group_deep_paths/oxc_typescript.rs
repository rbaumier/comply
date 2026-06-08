//! OxcCheck backend for elysia-group-deep-paths.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const ROUTE_METHODS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "options", "head", "all",
];

fn segment_count(path: &str) -> usize {
    path.split('/').filter(|s| !s.is_empty()).count()
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let method = member.property.name.as_str();
        if !ROUTE_METHODS.contains(&method) {
            return;
        }

        // First argument should be a string path.
        let Some(first_arg) = call.arguments.first() else { return };
        let Some(Expression::StringLiteral(path_lit)) = first_arg.as_expression() else { return };
        let unquoted = path_lit.value.as_str();
        if segment_count(unquoted) < 3 {
            return;
        }

        // Skip if inside a `.group()` call.
        let nodes = semantic.nodes();
        let mut current = node.id();
        loop {
            let parent_id = nodes.parent_id(current);
            if parent_id == current {
                break;
            }
            let parent = nodes.get_node(parent_id);
            if let AstKind::CallExpression(parent_call) = parent.kind()
                && let Expression::StaticMemberExpression(pm) = &parent_call.callee
                    && pm.property.name.as_str() == "group" {
                        return;
                    }
            current = parent_id;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Path `{unquoted}` has {} segments — consider grouping with `.group()` or a `prefix`.",
                segment_count(unquoted)
            ),
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
    fn flags_three_segment_path() {
        let src = "import { Elysia } from 'elysia';\napp.get('/v1/users/profile', handler);";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_four_segment_path() {
        let src = "import { Elysia } from 'elysia';\napp.post('/api/v2/users/me', handler);";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_shallow_path() {
        let src = "import { Elysia } from 'elysia';\napp.get('/users', handler);";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_grouped_routes() {
        let src = "import { Elysia } from 'elysia';\napp.group('/v1/users', g => g.get('/profile/edit/save', handler));";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.get('/v1/users/profile', handler);";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
