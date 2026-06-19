//! OxcCheck backend for no-inner-html — flag `.innerHTML = ...` / `.outerHTML = ...`.
//!
//! Two right-hand sides are exempt because they are provably non-dangerous:
//! - a `.__html` member access (`x.innerHTML = value.__html`) — the
//!   `dangerouslySetInnerHTML` implementation idiom, where the raw-HTML opt-in
//!   and escaping responsibility belong to the caller, not the renderer;
//! - an empty string literal (`x.innerHTML = ''` / `""`) — clearing a node
//!   cannot inject markup. A non-empty literal or a template literal stays flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
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
        if rhs_is_non_dangerous(&assign.right) {
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

/// A right-hand side that cannot be an XSS vector: the `.__html`
/// `dangerouslySetInnerHTML` idiom, or an empty string literal.
fn rhs_is_non_dangerous(right: &Expression) -> bool {
    match right {
        Expression::StaticMemberExpression(member) => member.property.name.as_str() == "__html",
        Expression::StringLiteral(lit) => lit.value.is_empty(),
        _ => false,
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    // FPs fixed: the dangerouslySetInnerHTML implementation idiom and clearing a node.
    #[test]
    fn allows_dangerously_set_inner_html_idiom() {
        assert!(run_on("container.innerHTML = value.__html;").is_empty());
    }

    #[test]
    fn allows_empty_single_quoted_string() {
        assert!(run_on("e.innerHTML = '';").is_empty());
    }

    #[test]
    fn allows_empty_double_quoted_string() {
        assert!(run_on("e.innerHTML = \"\";").is_empty());
    }

    // Core preserved: every dangerous right-hand side still flags.
    #[test]
    fn flags_dynamic_identifier() {
        assert_eq!(run_on("el.innerHTML = userInput;").len(), 1);
    }

    #[test]
    fn flags_template_literal() {
        assert_eq!(run_on("el.innerHTML = `<div>${x}</div>`;").len(), 1);
    }

    #[test]
    fn flags_non_empty_string_literal() {
        assert_eq!(run_on("el.innerHTML = \"<b>static</b>\";").len(), 1);
    }

    #[test]
    fn flags_outer_html_dynamic() {
        assert_eq!(run_on("el.outerHTML = userInput;").len(), 1);
    }
}
