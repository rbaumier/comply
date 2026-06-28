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
//!
//! An assignment whose target is a provable `<template>` element is exempt: a
//! template's content is an inert, off-document fragment that never executes
//! scripts, so `template.innerHTML = html` is the standard safe HTML parser, not
//! a live-DOM sink (see `html_sink_helpers::lhs_object_is_template_element`).
//!
//! An assignment to a detached `document.createElement(...)` element used only as
//! an HTML→text parser — every other reference reads `.textContent`/`.innerText`
//! and the element never reaches the live DOM — is also exempt (see
//! `oxc_helpers::assignment_target_is_detached_text_parser`).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{
    assignment_target_is_detached_text_parser, byte_offset_to_line_col,
    is_inside_browser_injection_callback,
};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::html_sink_helpers::{is_numeric_only_template, lhs_object_is_template_element};
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
        // Writing to a `<template>` element's `.innerHTML` is an inert HTML parse
        // (its content is an off-document fragment that never runs scripts), not
        // an XSS sink. Only provable `<template>` targets are exempt.
        if lhs_object_is_template_element(&member.object, semantic) {
            return;
        }
        // A detached element created by `document.createElement(...)` whose only
        // other references read `.textContent`/`.innerText` is an HTML→text
        // parser, never a live-DOM sink (see helper docs for the conservative
        // reference classification).
        if assignment_target_is_detached_text_parser(member, semantic) {
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

    // Repro for #5960: setting `.innerHTML` on a `<template>` element is an inert
    // HTML parse (off-document fragment, no script execution), not an XSS sink.
    #[test]
    fn allows_template_from_create_element_ns() {
        let src = "function f(html, ns) { const parser = (parsers[ns] ||= document.createElementNS(ns, \"template\")); parser.innerHTML = html; return parser.content; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_template_from_create_element() {
        let src = "function f(x) { const t = document.createElement(\"template\"); t.innerHTML = x; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_template_typed_binding() {
        let src = "function f(t: HTMLTemplateElement, x) { t.innerHTML = x; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_template_as_cast_target() {
        let src = "function f(el, x) { (el as HTMLTemplateElement).innerHTML = x; }";
        assert!(run_on(src).is_empty());
    }

    // Strong negatives: any non-template element keeps flagging.
    #[test]
    fn flags_div_from_create_element() {
        let src = "function f(x) { const d = document.createElement(\"div\"); d.innerHTML = x; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_script_from_create_element() {
        let src = "function f(x) { const s = document.createElement(\"script\"); s.innerHTML = x; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_unknown_param_element() {
        let src = "function f(el, x) { el.innerHTML = x; }";
        assert_eq!(run_on(src).len(), 1);
    }

    // Name-only evidence is never enough: a binding called `template` but created
    // as a `<div>` still flags.
    #[test]
    fn flags_div_named_template() {
        let src = "function f(x) { const template = document.createElement(\"div\"); template.innerHTML = x; }";
        assert_eq!(run_on(src).len(), 1);
    }

    // A `let` initialised from a `<template>` can be reassigned to a live-DOM
    // element, so the initializer proof must fail closed for non-`const` bindings.
    #[test]
    fn flags_reassignable_template_binding() {
        let src = "function f(x) { let t = document.createElement(\"template\"); t = document.createElement(\"div\"); t.innerHTML = x; }";
        assert_eq!(run_on(src).len(), 1);
    }

    // A member-chain receiver is never a provable `<template>`.
    #[test]
    fn flags_member_chain_receiver() {
        let src = "function f(x) { this.tmpl.innerHTML = x; }";
        assert_eq!(run_on(src).len(), 1);
    }

    // Repro for #6593: a detached `document.createElement('div')` used only as an
    // HTML→text parser (assign `.innerHTML`, then read `.textContent`) never
    // reaches the live DOM, so it is not an XSS sink.
    #[test]
    fn allows_detached_create_element_text_extraction() {
        let src = "function f(html) { const el = document.createElement('div'); el.innerHTML = html; const text = el.textContent; return text; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_detached_create_element_inner_text_read() {
        let src = "function f(html) { const el = document.createElement('span'); el.innerHTML = html; return el.innerText; }";
        assert!(run_on(src).is_empty());
    }

    // Negative controls: the element escapes to the live DOM, so it keeps flagging.
    #[test]
    fn flags_detached_element_appended_to_dom() {
        let src = "function f(html) { const el = document.createElement('div'); el.innerHTML = html; document.body.appendChild(el); }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_detached_element_with_append_child_receiver() {
        let src = "function f(html, child) { const el = document.createElement('div'); el.innerHTML = html; el.appendChild(child); }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_detached_element_returned() {
        let src = "function f(html) { const el = document.createElement('div'); el.innerHTML = html; return el; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_non_create_element_origin() {
        let src = "function f(html) { const el = document.getElementById('x'); el.innerHTML = html; const text = el.textContent; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_param_element_only_text_read() {
        let src = "function f(el, html) { el.innerHTML = html; const text = el.textContent; }";
        assert_eq!(run_on(src).len(), 1);
    }
}
