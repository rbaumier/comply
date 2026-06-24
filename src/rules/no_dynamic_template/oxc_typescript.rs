//! OXC backend for no-dynamic-template — flag dynamic HTML construction via
//! innerHTML/outerHTML assignments, document.write/insertAdjacentHTML calls, and
//! the dangerouslySetInnerHTML JSX attribute.
//!
//! Exemptions on innerHTML/outerHTML assignments: a compile-time-constant string
//! (a StringLiteral or a TemplateLiteral with no expressions), a template
//! literal whose every `${...}` is provably numeric (numbers cannot carry HTML
//! markup), and a write to a provable `<template>` element (its content is an
//! inert off-document fragment, the standard safe HTML parser, not a live-DOM
//! sink). Assignments and HTML-construction calls inside a Playwright/Puppeteer
//! injection callback (`page.evaluate(...)`) are also exempt: they run in a
//! controlled automation browser, not the application DOM.

use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::{byte_offset_to_line_col, is_inside_browser_injection_callback};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::html_sink_helpers::{is_numeric_only_template, lhs_object_is_template_element};
use oxc_ast::ast::{AssignmentTarget, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_static_string(expr: &Expression) -> bool {
    match expr {
        Expression::StringLiteral(_) => true,
        Expression::TemplateLiteral(tpl) => tpl.expressions.is_empty(),
        _ => false,
    }
}

const ASSIGNMENT_PROPS: &[&str] = &["innerHTML", "outerHTML"];
const CALL_METHODS: &[&str] = &[
    "document.write",
    "document.writeln",
    "insertAdjacentHTML",
    "createContextualFragment",
    "setHTMLUnsafe",
];

fn emit(ctx: &CheckCtx, start: u32, detail: &str, diagnostics: &mut Vec<Diagnostic>) {
    let (line, column) = byte_offset_to_line_col(ctx.source, start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: format!(
            "Dynamic HTML construction via `{detail}` — use safe DOM APIs or framework escaping instead."
        ),
        severity: super::META.severity,
        span: None,
    });
}

/// Get source text for a span.
fn span_text(source: &str, span: oxc_span::Span) -> &str {
    &source[span.start as usize..span.end as usize]
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::AssignmentExpression,
            AstType::CallExpression,
            AstType::JSXAttribute,
        ]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["innerHTML", "outerHTML", "document.write", "insertAdjacentHTML",
               "createContextualFragment", "setHTMLUnsafe", "dangerouslySetInnerHTML"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // HTML construction inside a Playwright/Puppeteer injection callback runs
        // in a controlled automation browser, not the application DOM — no XSS sink.
        if is_inside_browser_injection_callback(node, semantic) {
            return;
        }
        match node.kind() {
            AstKind::AssignmentExpression(assign) => {
                let (lhs_text, lhs_object) = match &assign.left {
                    AssignmentTarget::StaticMemberExpression(member) => {
                        (span_text(ctx.source, member.span), &member.object)
                    }
                    AssignmentTarget::ComputedMemberExpression(member) => {
                        (span_text(ctx.source, member.span), &member.object)
                    }
                    _ => return,
                };
                for prop in ASSIGNMENT_PROPS {
                    if lhs_text.ends_with(prop) {
                        // A compile-time-constant string (`el.innerHTML = '<div>Hello</div>'`)
                        // carries no dynamic or user-controlled content, so it is neither a
                        // dynamic template nor an XSS sink. Same exemption as
                        // `no-unsanitized-property`'s `is_static_string`.
                        if is_static_string(&assign.right) {
                            return;
                        }
                        // A template whose every interpolation is provably numeric cannot
                        // carry HTML markup, so it is not a dynamic-HTML sink.
                        if is_numeric_only_template(&assign.right) {
                            return;
                        }
                        // Writing to a `<template>` element's `.innerHTML` is an inert HTML
                        // parse (off-document fragment, no script execution), not a sink.
                        if lhs_object_is_template_element(lhs_object, semantic) {
                            return;
                        }
                        emit(ctx, assign.span.start, prop, diagnostics);
                        return;
                    }
                }
            }
            AstKind::CallExpression(call) => {
                let callee_text = span_text(ctx.source, call.callee.span());
                for method in CALL_METHODS {
                    if callee_text == *method || callee_text.ends_with(&format!(".{method}")) {
                        emit(ctx, call.span.start, method, diagnostics);
                        return;
                    }
                }
            }
            AstKind::JSXAttribute(attr) => {
                let name = match &attr.name {
                    oxc_ast::ast::JSXAttributeName::Identifier(id) => id.name.as_str(),
                    oxc_ast::ast::JSXAttributeName::NamespacedName(ns) => {
                        if ns.name.name.as_str() == "dangerouslySetInnerHTML" {
                            "dangerouslySetInnerHTML"
                        } else {
                            return;
                        }
                    }
                };
                if name == "dangerouslySetInnerHTML" {
                    emit(
                        ctx,
                        attr.span.start,
                        "dangerouslySetInnerHTML",
                        diagnostics,
                    );
                }
            }
            _ => {}
        }
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
    fn flags_innerhtml() {
        assert_eq!(run_on("el.innerHTML = '<b>' + name + '</b>';").len(), 1);
    }

    #[test]
    fn flags_document_write() {
        assert_eq!(
            run_on("document.write('<script>alert(1)</script>');").len(),
            1
        );
    }

    #[test]
    fn flags_insert_adjacent_html() {
        assert_eq!(run_on("el.insertAdjacentHTML('beforeend', html);").len(), 1);
    }

    #[test]
    fn allows_text_content() {
        assert!(run_on("el.textContent = name;").is_empty());
    }

    // Repro for #5559: `location.href = …` is URL navigation, not HTML injection.
    // It navigates the browser; no markup is parsed or written into the DOM.
    #[test]
    fn allows_location_href_assignment() {
        assert!(run_on("location.href = `https://x/${id}`;").is_empty());
        assert!(run_on("win.location.href = url;").is_empty());
        assert!(run_on("window.location.href = url;").is_empty());
    }

    #[test]
    fn allows_static_innerhtml_string() {
        assert!(run_on("scratch.innerHTML = '<div>Hello</div>';").is_empty());
        assert!(run_on("el.innerHTML = '<div></div>';").is_empty());
    }

    #[test]
    fn allows_static_innerhtml_template() {
        assert!(run_on("el.innerHTML = `<p>static</p>`;").is_empty());
    }

    #[test]
    fn allows_static_outerhtml_string() {
        assert!(run_on("el.outerHTML = '<span></span>';").is_empty());
    }

    #[test]
    fn flags_dynamic_innerhtml_concat() {
        assert_eq!(run_on("el.innerHTML = '<b>' + name + '</b>';").len(), 1);
    }

    #[test]
    fn flags_dynamic_innerhtml_template() {
        assert_eq!(run_on("el.innerHTML = `<p>${name}</p>`;").len(), 1);
    }

    #[test]
    fn flags_innerhtml_variable() {
        assert_eq!(run_on("el.innerHTML = userInput;").len(), 1);
    }

    // Numbers cannot carry HTML markup, so a numeric-only template is safe.
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

    // document.write inside an injection callback targets the automation browser.
    #[test]
    fn allows_document_write_inside_page_evaluate() {
        let src = "page.evaluate((html) => { document.write(html); }, html);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_inner_html_outside_injection_callback() {
        let src = "function f(html) { el.innerHTML = `<div>${html}</div>`; }";
        assert_eq!(run_on(src).len(), 1);
    }

    // Repro for #5960: `.innerHTML` on a `<template>` element is an inert HTML
    // parse, not a dynamic-HTML sink.
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
    fn flags_unknown_param_element() {
        let src = "function f(el, x) { el.innerHTML = x; }";
        assert_eq!(run_on(src).len(), 1);
    }

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
}
