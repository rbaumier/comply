//! OxcCheck backend for no-inner-html — flag `.innerHTML = ...` / `.outerHTML = ...`.
//!
//! Right-hand sides exempt because they are provably non-dangerous:
//! - a `.__html` member access (`x.innerHTML = value.__html`) — the
//!   `dangerouslySetInnerHTML` implementation idiom, where the raw-HTML opt-in
//!   and escaping responsibility belong to the caller, not the renderer;
//! - an empty string literal (`x.innerHTML = ''` / `""`) — clearing a node
//!   cannot inject markup. A non-empty literal stays flagged;
//! - a template literal whose every `${...}` is provably numeric — numbers
//!   cannot carry HTML markup. A string/unknown interpolation stays flagged.
//!
//! Assignments inside a Playwright/Puppeteer injection callback
//! (`page.evaluate(() => { el.innerHTML = ... })`) are also exempt: the callback
//! runs in a controlled automation browser, not an application XSS sink.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, is_inside_browser_injection_callback};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::html_sink_helpers::is_numeric_only_template;
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
        semantic: &'a oxc_semantic::Semantic<'a>,
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
        if is_numeric_only_template(&assign.right) {
            return;
        }
        if is_inside_browser_injection_callback(node, semantic) {
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

    // Numbers cannot carry HTML markup, so a numeric-only template is safe.
    #[test]
    fn allows_numeric_only_template() {
        assert!(run_on("el.innerHTML = `left: ${10}px; top: ${20}px`;").is_empty());
    }

    #[test]
    fn allows_arithmetic_only_template() {
        assert!(run_on("el.innerHTML = `width: ${10 * 2}px; n: ${items.length}`;").is_empty());
    }

    // A string interpolation outside any injection callback must still flag.
    #[test]
    fn flags_string_interpolation_template() {
        assert_eq!(run_on("el.innerHTML = `<b>${userString}</b>`;").len(), 1);
    }

    // Repro for #5541: numeric interpolations inside a Puppeteer page.evaluate()
    // injection callback target a controlled automation browser, not an app DOM.
    #[test]
    fn allows_inner_html_inside_page_evaluate() {
        let src = "page.evaluate((x, y) => { const h = document.createElement('div'); h.innerHTML = `<style>:scope { left: ${x}px; top: ${y}px; }</style>`; }, x, y);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_inner_html_outside_injection_callback() {
        let src = "function f(html) { el.innerHTML = `<div>${html}</div>`; }";
        assert_eq!(run_on(src).len(), 1);
    }
}
