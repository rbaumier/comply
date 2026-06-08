//! prefer-query-selector oxc backend — flag legacy DOM query methods.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const METHODS: &[(&str, &str)] = &[
    ("getElementById", "querySelector"),
    ("getElementsByClassName", "querySelectorAll"),
    ("getElementsByTagName", "querySelectorAll"),
    ("getElementsByName", "querySelectorAll"),
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["getElementById", "getElementsByClassName", "getElementsByTagName", "getElementsByName"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let method_name = member.property.name.as_str();

        let Some((_, replacement)) = METHODS.iter().find(|(m, _)| *m == method_name) else { return };

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("Prefer `.{replacement}()` over `.{method_name}()`."),
            severity: Severity::Warning,
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
    fn flags_get_element_by_id() {
        let d = run_on(r#"document.getElementById("foo");"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("querySelector"));
    }


    #[test]
    fn flags_get_elements_by_class_name() {
        let d = run_on(r#"document.getElementsByClassName("bar");"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("querySelectorAll"));
    }


    #[test]
    fn flags_get_elements_by_tag_name() {
        let d = run_on(r#"document.getElementsByTagName("div");"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("querySelectorAll"));
    }


    #[test]
    fn allows_query_selector() {
        assert!(run_on(r##"document.querySelector("#foo");"##).is_empty());
    }
}
