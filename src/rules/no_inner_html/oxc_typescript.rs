//! OxcCheck backend for no-inner-html — flag `.innerHTML = ...` / `.outerHTML = ...`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["innerHTML", "outerHTML"])
    }

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
        let AstKind::AssignmentExpression(assign) = node.kind() else { return };
        let oxc_ast::ast::AssignmentTarget::StaticMemberExpression(member) = &assign.left else {
            return;
        };
        let prop = member.property.name.as_str();
        if prop != "innerHTML" && prop != "outerHTML" {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, assign.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Writing to `.{prop}` is an XSS sink — use `textContent` or sanitize via DOMPurify."
            ),
            severity: Severity::Error,
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
    fn flags_inner_html_assignment() {
        assert_eq!(run_on("el.innerHTML = raw;").len(), 1);
    }


    #[test]
    fn flags_outer_html_assignment() {
        assert_eq!(run_on("el.outerHTML = raw;").len(), 1);
    }


    #[test]
    fn flags_inner_html_plus_equals() {
        assert_eq!(run_on("el.innerHTML += raw;").len(), 1);
    }


    #[test]
    fn allows_text_content_assignment() {
        assert!(run_on("el.textContent = raw;").is_empty());
    }


    #[test]
    fn allows_reading_inner_html() {
        assert!(run_on("const s = el.innerHTML;").is_empty());
    }
}
