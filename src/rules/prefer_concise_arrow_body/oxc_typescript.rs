//! prefer-concise-arrow-body oxc backend — flag block-bodied arrows whose
//! body is exactly one `return <expr>;` statement.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, Statement};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ArrowFunctionExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ArrowFunctionExpression(arrow) = node.kind() else {
            return;
        };

        // Already in concise form.
        if arrow.expression {
            return;
        }

        let body = &arrow.body;

        if body.statements.len() != 1 {
            return;
        }

        let Statement::ReturnStatement(ret) = &body.statements[0] else {
            return;
        };

        // Bare `return;` with no argument — skip.
        let Some(argument) = &ret.argument else {
            return;
        };

        // Skip when any comment lives inside the block braces.
        let has_comment = semantic.comments().iter().any(|c| {
            c.span.start > body.span.start && c.span.end < body.span.end
        });
        if has_comment {
            return;
        }

        let message = if matches!(argument, Expression::ObjectExpression(_)) {
            "Arrow function body can be collapsed to concise form; wrap object literal in parentheses: `() => ({...})`"
        } else {
            "Arrow function body contains a single `return` — use concise body: `() => expr`"
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, arrow.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: message.into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_simple_return() {
        let diags = run("const f = () => { return expr; };");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("use concise body"));
    }

    #[test]
    fn flags_map_callback() {
        let diags = run("arr.map((x) => { return x.field; });");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_object_literal_with_paren_wrap_hint() {
        let diags = run("const f = () => { return { a: 1 }; };");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("parentheses"));
    }

    #[test]
    fn skips_block_with_comment() {
        let diags = run("const f = () => { /* comment */ return x; };");
        assert!(diags.is_empty(), "should not flag when comment present: {:?}", diags);
    }

    #[test]
    fn skips_bare_return() {
        let diags = run("const f = () => { return; };");
        assert!(diags.is_empty());
    }

    #[test]
    fn skips_already_concise() {
        let diags = run("const f = () => expr;");
        assert!(diags.is_empty());
    }

    #[test]
    fn skips_multi_statement_block() {
        let diags = run("const f = () => { doSomething(); return x; };");
        assert!(diags.is_empty());
    }

    // Regression: attach-gamme-product.ts:39 shape
    #[test]
    fn regression_field_access_return() {
        let diags = run("const f = (x) => { return x.field; };");
        assert_eq!(diags.len(), 1);
    }
}
