//! OXC backend for no-unsanitized-property — flag unsafe assignments to
//! innerHTML/outerHTML/srcdoc.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{AssignmentTarget, Expression};
use std::sync::Arc;

fn is_static_string(expr: &Expression) -> bool {
    match expr {
        Expression::StringLiteral(_) => true,
        Expression::TemplateLiteral(tpl) => tpl.expressions.is_empty(),
        _ => false,
    }
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
        _semantic: &'a oxc_semantic::Semantic<'a>,
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
    fn allows_compound_assignment() {
        let src = "el.innerHTML += extra;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_unrelated_property() {
        let src = "el.textContent = userInput;";
        assert!(run_on(src).is_empty());
    }
}
