//! OxcCheck backend for playwright-no-unsafe-references.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::{GetSpan, Span};
use std::sync::Arc;

pub struct Check;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_test_file(ctx.path) {
            return;
        }
        if !crate::rules::playwright::is_playwright_context(ctx) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "evaluate" {
            return;
        }

        // Receiver must be `page`.
        let Expression::Identifier(obj) = &member.object else { return };
        if obj.name.as_str() != "page" {
            return;
        }

        // Must have exactly one argument and it must be a function.
        if call.arguments.len() != 1 {
            return;
        }
        let Some(arg_expr) = call.arguments[0].as_expression() else { return };
        let callback_scope = match arg_expr {
            Expression::ArrowFunctionExpression(arrow) => arrow.scope_id.get(),
            Expression::FunctionExpression(func) => func.scope_id.get(),
            _ => return,
        };
        let Some(callback_scope) = callback_scope else { return };

        // Only flag when the callback actually closes over an outer-scope
        // binding. A self-contained callback (browser globals + its own
        // params/locals only) is serialized intact into the browser and is
        // safe; the second argument would be misleading there.
        if !captures_outer_symbol(semantic, callback_scope, arg_expr.span()) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`page.evaluate()` callback captures an outer-scope \
                      variable — pass it as the second argument."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True when the callback whose scope is `callback_scope` (spanning
/// `callback_span`) reads at least one identifier that resolves to a binding
/// declared in an enclosing scope — a captured free variable.
///
/// Walks the callback's ancestor scopes (its enclosing functions and the
/// module/root scope), then for every symbol declared in those scopes checks
/// whether any of its resolved references falls inside the callback span. The
/// callback's own parameters and `let`/`const` declarations live in descendant
/// scopes and are excluded; browser globals (`document`, `window`, …) resolve
/// to no symbol and never appear here.
fn captures_outer_symbol(
    semantic: &oxc_semantic::Semantic,
    callback_scope: oxc_semantic::ScopeId,
    callback_span: Span,
) -> bool {
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();

    let mut ancestor_scopes: Vec<oxc_semantic::ScopeId> = Vec::new();
    let mut cursor = scoping.scope_parent_id(callback_scope);
    while let Some(scope) = cursor {
        ancestor_scopes.push(scope);
        cursor = scoping.scope_parent_id(scope);
    }

    for symbol_id in scoping.symbol_ids() {
        if !ancestor_scopes.contains(&scoping.symbol_scope_id(symbol_id)) {
            continue;
        }
        for reference in scoping.get_resolved_references(symbol_id) {
            let ref_span = nodes.kind(reference.node_id()).span();
            if ref_span.start >= callback_span.start && ref_span.end <= callback_span.end {
                return true;
            }
        }
    }
    false
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
        let full = format!("import {{ test, expect }} from \"@playwright/test\";\n{source}");
        crate::rules::test_helpers::run_rule(&Check, &full, "login.spec.ts")
    }

    #[test]
    fn allows_self_contained_arrow() {
        // Regression for rbaumier/comply#2271 — captures nothing.
        let d = run_on("await page.evaluate(() => document.title);");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_self_contained_arrow_with_locals() {
        // Regression for rbaumier/comply#2271 — only local bindings.
        let d = run_on("await page.evaluate(() => { const x = 1; return x; });");
        assert!(d.is_empty());
    }

    #[test]
    fn flags_capture_of_outer_variable() {
        let d = run_on("const sel = '.foo'; await page.evaluate(() => document.querySelector(sel));");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-unsafe-references");
    }

    #[test]
    fn allows_capture_passed_as_second_arg() {
        let d = run_on(
            "const sel = '.foo'; await page.evaluate((s) => document.querySelector(s), sel);",
        );
        assert!(d.is_empty());
    }

    #[test]
    fn allows_evaluate_with_string_arg() {
        let d = run_on("await page.evaluate('document.title');");
        assert!(d.is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        let d = crate::rules::test_helpers::run_rule(
            &Check,
            "await page.evaluate(() => document.querySelector(window.x));",
            "helpers.ts",
        );
        assert!(d.is_empty());
    }
}
