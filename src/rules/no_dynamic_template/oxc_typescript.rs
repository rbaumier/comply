//! OXC backend for no-dynamic-template — flag dynamic HTML construction via
//! innerHTML/outerHTML assignments, document.write/insertAdjacentHTML calls, and
//! the dangerouslySetInnerHTML JSX attribute.
//!
//! A compile-time-constant string assigned to innerHTML/outerHTML (a
//! StringLiteral or a TemplateLiteral with no expressions) is exempt: it carries
//! no dynamic or user-controlled content, so it is neither a dynamic template nor
//! an XSS sink.

use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
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
               "createContextualFragment", "setHTMLUnsafe", "dangerouslySetInnerHTML",
               "location.href"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::AssignmentExpression(assign) => {
                let lhs_text = match &assign.left {
                    AssignmentTarget::StaticMemberExpression(member) => {
                        span_text(ctx.source, member.span)
                    }
                    AssignmentTarget::ComputedMemberExpression(member) => {
                        span_text(ctx.source, member.span)
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
                        emit(ctx, assign.span.start, prop, diagnostics);
                        return;
                    }
                }
                if lhs_text.ends_with("location.href") || lhs_text == "location.href" {
                    emit(ctx, assign.span.start, "location.href =", diagnostics);
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

    #[test]
    fn flags_location_href() {
        assert_eq!(run_on("location.href = userInput;").len(), 1);
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
}
