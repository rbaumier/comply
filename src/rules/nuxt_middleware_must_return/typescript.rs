//! nuxt-middleware-must-return backend.
//!
//! Inspects the body of `defineNuxtRouteMiddleware((to, from) => { ... })`
//! and flags `return <expr>` statements where `<expr>` is not a call to
//! `navigateTo` / `abortNavigation`. A bare value return is treated as
//! "continue", which is fine — but returning a string, number, or boolean
//! is almost always a bug (often confused with Express/Koa middleware).

use crate::diagnostic::{Diagnostic, Severity};

fn is_nav_call(text: &str) -> bool {
    let t = text.trim();
    t.starts_with("navigateTo(")
        || t.starts_with("abortNavigation(")
        || t.starts_with("await navigateTo(")
        || t.starts_with("await abortNavigation(")
}

crate::ast_check! { on ["return_statement"] prefilter = ["defineNuxtRouteMiddleware"] => |node, source, ctx, diagnostics|
    let mut p = node.parent();
    let mut in_middleware = false;
    let mut depth = 0;
    while let Some(parent) = p {
        if parent.kind() == "call_expression" {
            if let Some(callee) = parent.child_by_field_name("function") {
                if let Ok(name) = callee.utf8_text(source) {
                    if name == "defineNuxtRouteMiddleware" {
                        in_middleware = true;
                        break;
                    }
                }
            }
        }
        depth += 1;
        if depth > 10 {
            return;
        }
        p = parent.parent();
    }
    if !in_middleware {
        return;
    }

    let mut cursor = node.walk();
    let mut return_value: Option<tree_sitter::Node> = None;
    for child in node.children(&mut cursor) {
        if child.kind() == "return" {
            continue;
        }
        if child.is_named() {
            return_value = Some(child);
            break;
        }
    }
    let Some(expr) = return_value else { return };
    let Ok(text) = expr.utf8_text(source) else { return };
    if is_nav_call(text) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf().into(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "nuxt-middleware-must-return".into(),
        message: "Route middleware must return `navigateTo(...)`, `abortNavigation(...)`, or nothing.".into(),
        severity: Severity::Error,
        span: Some((node.byte_range().start, node.byte_range().len())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
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
