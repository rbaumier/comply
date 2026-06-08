//! hono-csrf-missing oxc backend — flag mutation routes without CSRF protection.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const MUTATION_METHODS: &[&str] = &["post", "put", "delete", "patch"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["hono"])
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

        // Only check Hono files.
        if !ctx.source_contains("from 'hono'") && !ctx.source_contains("from \"hono\"") {
            return;
        }

        // Skip if CSRF protection is already imported.
        if ctx.source_contains("hono/csrf") {
            return;
        }

        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method_name = member.property.name.as_str();
        if !MUTATION_METHODS.contains(&method_name) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Mutation route without CSRF protection — add `app.use(csrf())`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_post_without_csrf() {
        let src =
            "import { Hono } from 'hono';\nconst app = new Hono();\napp.post('/api', handler);";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_multiple_mutation_routes() {
        let src = "import { Hono } from 'hono';\napp.post('/a', h);\napp.put('/b', h);\napp.delete('/c', h);";
        assert_eq!(run_on(src).len(), 3);
    }


    #[test]
    fn allows_with_csrf_import() {
        let src = "import { Hono } from 'hono';\nimport { csrf } from 'hono/csrf';\napp.use(csrf());\napp.post('/api', handler);";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_hono_files() {
        let src = "app.post('/api', handler);";
        assert!(run_on(src).is_empty());
    }
}
