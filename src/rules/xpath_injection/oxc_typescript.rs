//! xpath-injection oxc backend — flag dynamic XPath queries.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, BinaryOperator, Expression};
use std::sync::Arc;

const XPATH_METHODS: &[&str] = &[
    "select",
    "select1",
    "evaluate",
    "selectNodes",
    "selectSingleNode",
];

/// Last identifier segment of a member expression's receiver, e.g. `scope` for
/// `state.scope.evaluate(...)` or `document` for `document.evaluate(...)`.
/// Returns `None` for receivers without a trailing name (calls, `this`, etc.).
fn receiver_name<'a>(object: &'a Expression<'a>) -> Option<&'a str> {
    match object {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(m) => Some(m.property.name.as_str()),
        _ => None,
    }
}

/// `evaluate` is a common method name on non-XPath APIs (AST/scope evaluators,
/// Playwright `page.evaluate`, expression evaluators). Only treat it as the DOM
/// XPath API (`document.evaluate`) or an `xpath`-package evaluator when the
/// receiver name signals XPath; the other method names in `XPATH_METHODS` are
/// XPath-specific enough to fire on their own.
fn is_xpath_receiver(object: &Expression) -> bool {
    let Some(name) = receiver_name(object) else { return false };
    name == "document" || name == "xpath" || name.to_ascii_lowercase().contains("xpath")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["evaluate", "selectNodes", "selectSingleNode"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be a member expression (e.g. xpath.select, doc.evaluate)
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let method_name = member.property.name.as_str();
        if !XPATH_METHODS.contains(&method_name) {
            return;
        }

        // `evaluate` is ambiguous: only flag it when the receiver looks like an
        // XPath processor, otherwise it matches AST/scope evaluators and the like.
        if method_name == "evaluate" && !is_xpath_receiver(&member.object) {
            return;
        }

        // Must have at least one argument
        let Some(first_arg) = call.arguments.first() else { return };

        // Flag if first argument (XPath query) is dynamic
        let is_dynamic = match first_arg {
            Argument::TemplateLiteral(tpl) => !tpl.expressions.is_empty(),
            Argument::BinaryExpression(bin) => bin.operator == BinaryOperator::Addition,
            Argument::Identifier(_)
            | Argument::StaticMemberExpression(_)
            | Argument::ComputedMemberExpression(_) => true,
            _ => false,
        };

        if !is_dynamic {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "XPath query with dynamic input — potential XPath injection.".into(),
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
    fn run(code: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, code, "t.ts")
    }

    #[test]
    fn flags_dom_evaluate_with_dynamic_query() {
        assert_eq!(run("document.evaluate(query, doc)").len(), 1);
    }

    #[test]
    fn flags_xpath_package_evaluate() {
        assert_eq!(run("xpath.evaluate(expr, doc)").len(), 1);
    }

    #[test]
    fn flags_select_nodes_template() {
        assert_eq!(run("dom.selectNodes(`//user[@name='${name}']`)").len(), 1);
    }

    #[test]
    fn flags_select_single_node_concat() {
        assert_eq!(run("dom.selectSingleNode('//user[@id=' + id + ']')").len(), 1);
    }

    #[test]
    fn allows_static_dom_evaluate() {
        assert!(run("document.evaluate('//user', doc)").is_empty());
    }

    // Regression for #1763: `.evaluate()` on a non-XPath receiver (Svelte's
    // compiler-internal Scope) is an AST evaluation, not an XPath query.
    #[test]
    fn allows_scope_evaluate() {
        assert!(run("const evaluated = scope.evaluate(expression);").is_empty());
    }

    #[test]
    fn allows_nested_scope_evaluate() {
        assert!(run("const evaluated = state.scope.evaluate(node.expression);").is_empty());
    }

    #[test]
    fn allows_playwright_page_evaluate() {
        assert!(run("page.evaluate(selectorFn)").is_empty());
    }
}
