//! no-single-promise-in-promise-methods OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ArrayExpressionElement, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

const PROMISE_METHODS: &[&str] = &["all", "any", "race"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Promise"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be `Promise.<method>`
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let Expression::Identifier(obj) = &member.object else { return };
        if obj.name.as_str() != "Promise" {
            return;
        }
        let method_name = member.property.name.as_str();
        if !PROMISE_METHODS.contains(&method_name) {
            return;
        }

        // First argument must be an array literal with exactly one non-spread element
        let Some(first) = call.arguments.first() else { return };
        let Some(Expression::ArrayExpression(arr)) = first.as_expression() else { return };
        if arr.elements.len() != 1 {
            return;
        }
        // Reject spread elements
        if matches!(arr.elements[0], ArrayExpressionElement::SpreadElement(_)) {
            return;
        }

        let span = arr.span();
        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Wrapping single-element array with `Promise.{method_name}()` is unnecessary \
                 \u{2014} use the value directly."
            ),
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
    fn flags_promise_all_single() {
        let d = run_on("await Promise.all([fetchData()]);");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-single-promise-in-promise-methods");
    }


    #[test]
    fn flags_promise_race_single() {
        let d = run_on("await Promise.race([fetchData()]);");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_promise_any_single() {
        let d = run_on("await Promise.any([fetchData()]);");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_multiple_elements() {
        assert!(run_on("await Promise.all([fetchA(), fetchB()]);").is_empty());
    }


    #[test]
    fn allows_spread_element() {
        assert!(run_on("await Promise.all([...promises]);").is_empty());
    }


    #[test]
    fn allows_promise_all_settled() {
        // allSettled is not in the list — semantics differ
        assert!(run_on("await Promise.allSettled([fetchData()]);").is_empty());
    }


    #[test]
    fn allows_empty_array() {
        assert!(run_on("await Promise.all([]);").is_empty());
    }
}
