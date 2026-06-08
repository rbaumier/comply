//! OxcCheck backend for nuxt-middleware-must-return.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

fn is_nav_call(src: &str, expr: &Expression) -> bool {
    match expr {
        Expression::CallExpression(call) => {
            let name = match &call.callee {
                Expression::Identifier(id) => id.name.as_str(),
                _ => return false,
            };
            name == "navigateTo" || name == "abortNavigation"
        }
        Expression::AwaitExpression(aw) => is_nav_call(src, &aw.argument),
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ReturnStatement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["defineNuxtRouteMiddleware"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ReturnStatement(ret) = node.kind() else { return };

        // Bare return is fine.
        let Some(arg) = &ret.argument else { return };

        // Check if return value is navigateTo/abortNavigation.
        if is_nav_call(ctx.source, arg) {
            return;
        }

        // Walk parents to check if we're inside defineNuxtRouteMiddleware.
        let mut in_middleware = false;
        let mut current = node.id();
        let nodes = semantic.nodes();
        let mut depth = 0;
        loop {
            let parent_id = nodes.parent_id(current);
            if parent_id == current {
                break;
            }
            let parent = nodes.get_node(parent_id);
            if let AstKind::CallExpression(call) = parent.kind() {
                if let Expression::Identifier(callee) = &call.callee {
                    if callee.name.as_str() == "defineNuxtRouteMiddleware" {
                        in_middleware = true;
                        break;
                    }
                }
            }
            current = parent_id;
            depth += 1;
            if depth > 10 {
                return;
            }
        }
        if !in_middleware {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, ret.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Route middleware must return `navigateTo(...)`, `abortNavigation(...)`, or nothing.".into(),
            severity: Severity::Error,
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
    fn flags_bare_return_value() {
        let src = "export default defineNuxtRouteMiddleware((to) => { if (!to.params.id) return false; });";
        assert!(!run_on(src).is_empty());
    }


    #[test]
    fn allows_navigate_to() {
        let src = "export default defineNuxtRouteMiddleware((to) => { if (!to.params.id) return navigateTo('/'); });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_abort_navigation() {
        let src = "export default defineNuxtRouteMiddleware(() => { return abortNavigation(); });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_bare_return() {
        let src = "export default defineNuxtRouteMiddleware(() => { return; });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_middleware_functions() {
        let src = "function helper() { return 42; }";
        assert!(run_on(src).is_empty());
    }
}
