//! no-await-in-promise-methods OxcCheck backend — flag `await` inside
//! `Promise.all/race/any/allSettled` arrays.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

const PROMISE_METHODS: &[&str] = &["all", "allSettled", "any", "race"];

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
        if obj.name != "Promise" {
            return;
        }
        let method_name = member.property.name.as_str();
        if !PROMISE_METHODS.contains(&method_name) {
            return;
        }

        // First argument must be an array
        let Some(first_arg) = call.arguments.first() else { return };
        if call.arguments.len() != 1 {
            return;
        }
        let oxc_ast::ast::Argument::ArrayExpression(arr) = first_arg else { return };

        // Walk array elements looking for AwaitExpression
        for element in &arr.elements {
            let oxc_ast::ast::ArrayExpressionElement::AwaitExpression(await_expr) = element else {
                continue;
            };
            let (line, column) =
                byte_offset_to_line_col(ctx.source, await_expr.span().start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Promise in `Promise.{method_name}()` should not be awaited \
                     — this serializes the calls."
                ),
                severity: Severity::Warning,
                span: None,
            });
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
    fn flags_await_in_promise_all() {
        let d = run_on("await Promise.all([await fetchA(), await fetchB()]);");
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].rule_id, "no-await-in-promise-methods");
    }


    #[test]
    fn flags_single_await_in_promise_race() {
        let d = run_on("await Promise.race([await fetchA(), fetchB()]);");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_await_in_promise_all_settled() {
        let d = run_on("await Promise.allSettled([await a(), await b()]);");
        assert_eq!(d.len(), 2);
    }


    #[test]
    fn flags_await_in_promise_any() {
        let d = run_on("await Promise.any([await fetchA()]);");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_no_await_in_promise_all() {
        assert!(run_on("await Promise.all([fetchA(), fetchB()]);").is_empty());
    }


    #[test]
    fn allows_promise_resolve() {
        assert!(run_on("await Promise.resolve(42);").is_empty());
    }


    #[test]
    fn allows_non_promise_call() {
        assert!(run_on("foo([await bar()]);").is_empty());
    }
}
