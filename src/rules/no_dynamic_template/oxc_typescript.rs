use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::AssignmentTarget;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

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
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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
}
