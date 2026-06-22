//! OXC backend for no-unsanitized-property — flag unsafe assignments to
//! innerHTML/outerHTML/srcdoc.
//!
//! Exempt right-hand sides: a static string literal; the `.__html`
//! `dangerouslySetInnerHTML` idiom (`el.innerHTML = x.__html`), where the
//! `.__html` member access is the React/Preact/Hono opt-in marker so the caller
//! owns sanitization; and a template literal whose every `${...}` is provably
//! numeric, since numbers cannot carry HTML markup. Assignments inside a
//! Playwright/Puppeteer injection callback (`page.evaluate(...)`) are exempt too:
//! they run in a controlled automation browser, not an application XSS sink.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, is_inside_browser_injection_callback};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::html_sink_helpers::is_numeric_only_template;
use oxc_ast::ast::{AssignmentTarget, Expression};
use std::sync::Arc;

fn is_static_string(expr: &Expression) -> bool {
    match expr {
        Expression::StringLiteral(_) => true,
        Expression::TemplateLiteral(tpl) => tpl.expressions.is_empty(),
        _ => false,
    }
}

/// True when the RHS is the `dangerouslySetInnerHTML` opt-in: `x.__html`. The
/// `.__html` member access is the deliberate danger marker used by React-style
/// renderers, so it is a controlled assignment, not an accidental XSS sink.
fn rhs_is_dangerously_set_inner_html(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::StaticMemberExpression(member) if member.property.name.as_str() == "__html"
    )
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AssignmentExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["innerHTML", "outerHTML", "srcdoc"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::AssignmentExpression(assign) = node.kind() else { return };

        // Only plain `=` (not `+=`, etc.)
        if assign.operator != oxc_ast::ast::AssignmentOperator::Assign {
            return;
        }

        // Left-hand side must be a static member expression like `el.innerHTML`
        let AssignmentTarget::StaticMemberExpression(member) = &assign.left else {
            return;
        };

        let prop_name = member.property.name.as_str();
        if !matches!(prop_name, "innerHTML" | "outerHTML" | "srcdoc") {
            return;
        }

        if is_static_string(&assign.right) {
            return;
        }

        // The `dangerouslySetInnerHTML` idiom (`el.innerHTML = x.__html`): the `.__html`
        // member access is React/Preact/Hono's explicit opt-in marker — the caller owns
        // sanitization — so it is not an accidental unsanitized assignment. Same
        // exemption as `no-inner-html`'s `rhs_is_non_dangerous`.
        if rhs_is_dangerously_set_inner_html(&assign.right) {
            return;
        }

        // A template whose every interpolation is provably numeric cannot inject
        // markup (numbers serialize to digits), so it is not unsanitized.
        if is_numeric_only_template(&assign.right) {
            return;
        }

        // Inside a Playwright/Puppeteer injection callback the assignment runs in
        // a controlled automation browser, not the application DOM — no XSS sink.
        if is_inside_browser_injection_callback(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, assign.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "no-unsanitized-property".into(),
            message: format!(
                "Assigning a non-literal value to `{prop_name}` is an XSS vector \u{2014} use textContent or sanitize the HTML first."
            ),
            severity: Severity::Error,
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_inner_html_variable() {
        let src = "el.innerHTML = userInput;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_outer_html_call() {
        let src = "el.outerHTML = getHtml();";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_srcdoc_concat() {
        let src = "frame.srcdoc = \"<p>\" + name + \"</p>\";";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_interpolated_template() {
        let src = "el.innerHTML = `<p>${name}</p>`;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_string_literal() {
        let src = "el.innerHTML = \"<p>static</p>\";";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_static_template() {
        let src = "el.innerHTML = `<p>static</p>`;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_dangerously_set_inner_html_idiom() {
        // The `.__html` opt-in marker: caller owns sanitization (preactjs/preact, #4481).
        assert!(run_on("dom.innerHTML = newHtml.__html;").is_empty());
        assert!(run_on("el.outerHTML = value.__html;").is_empty());
    }

    #[test]
    fn flags_non_html_member_access() {
        // Member access other than `.__html` is still an unsanitized sink.
        assert_eq!(run_on("el.innerHTML = value.html;").len(), 1);
        assert_eq!(run_on("el.innerHTML = value.innerHTML;").len(), 1);
    }

    #[test]
    fn allows_compound_assignment() {
        let src = "el.innerHTML += extra;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_unrelated_property() {
        let src = "el.textContent = userInput;";
        assert!(run_on(src).is_empty());
    }

    /// XSS has no attack surface in test code (jsdom SSR/hydration setup), so
    /// the rule is skipped in test files via `skip_in_test_dir`. Regression for
    /// emotion-js hydration tests that assign `innerHTML` to simulate
    /// server-rendered markup (issue #1963).
    #[test]
    fn skips_inner_html_in_test_file() {
        let src = "safeQuerySelector('body').innerHTML = `<style data-emotion=\"css ${hash}\">.css-${hash}{${css}}</style>`;";
        let diagnostics = crate::rules::test_helpers::run_rule_gated(
            &Check,
            src,
            "packages/cache/__tests__/hydration.js",
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn flags_inner_html_in_production_file() {
        let src = "safeQuerySelector('body').innerHTML = `<style data-emotion=\"css ${hash}\">.css-${hash}{${css}}</style>`;";
        let diagnostics = crate::rules::test_helpers::run_rule_gated(&Check, src, "src/app.ts");
        assert_eq!(diagnostics.len(), 1);
    }

    // Numeric-only interpolations cannot inject markup.
    #[test]
    fn allows_numeric_only_template() {
        assert!(run_on("el.innerHTML = `left: ${10}px; n: ${items.length}`;").is_empty());
    }

    // A string interpolation outside any injection callback must still flag.
    #[test]
    fn flags_string_interpolation_template() {
        assert_eq!(run_on("el.innerHTML = `<b>${userString}</b>`;").len(), 1);
    }

    // Repro for #5541: numeric interpolations inside a Puppeteer page.evaluate().
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
