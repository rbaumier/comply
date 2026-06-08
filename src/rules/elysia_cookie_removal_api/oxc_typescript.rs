use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AssignmentExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let AstKind::AssignmentExpression(assign) = node.kind() else { return };

        let left_span = assign.left.span();
        let left_text = &ctx.source[left_span.start as usize..left_span.end as usize];
        if !left_text.starts_with("cookie.") || !left_text.ends_with(".value") {
            return;
        }

        let right_span = assign.right.span();
        let right_text = ctx.source[right_span.start as usize..right_span.end as usize].trim();
        let is_empty_string = right_text == "''" || right_text == "\"\"" || right_text == "``";
        let is_null = right_text == "null" || right_text == "undefined";
        if !is_empty_string && !is_null {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, assign.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("`{left_text} = {right_text}` does not clear the cookie — call `cookie.<name>.remove()` instead."),
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
    fn flags_empty_string_assignment() {
        let src = "import { Elysia } from 'elysia';\napp.get('/logout', ({ cookie }) => { cookie.session.value = ''; });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_null_assignment() {
        let src = "import { Elysia } from 'elysia';\napp.get('/logout', ({ cookie }) => { cookie.session.value = null; });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_remove_call() {
        let src = "import { Elysia } from 'elysia';\napp.get('/logout', ({ cookie }) => { cookie.session.remove(); });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "cookie.session.value = '';";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
