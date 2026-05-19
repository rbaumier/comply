//! no-typeof-undefined OxcCheck backend — flag `typeof x === 'undefined'`
//! when `x` is a property access (safe to rewrite to `x === undefined`).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, UnaryOperator};
use std::sync::Arc;

pub struct Check;

/// Browser/DOM globals that are legitimately absent at SSR runtime.
/// `typeof X === 'undefined'` is the canonical SSR-detection idiom for these,
/// and TypeScript's lib.dom types declare them as never-undefined, so
/// `=== undefined` would be flagged by `no-unnecessary-condition`.
const DOM_GLOBALS: &[&str] = &[
    "window",
    "document",
    "navigator",
    "location",
    "history",
    "localStorage",
    "sessionStorage",
    "IntersectionObserver",
    "ResizeObserver",
    "MutationObserver",
    "requestAnimationFrame",
    "MediaQueryList",
    "matchMedia",
    "crypto",
    "performance",
    "indexedDB",
    "WebSocket",
    "Worker",
    "SharedWorker",
];

/// Global object names that proxy to the same set of properties.
const GLOBAL_PROXIES: &[&str] = &["globalThis", "window", "self", "global"];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BinaryExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["typeof"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::BinaryExpression(bin) = node.kind() else { return };

        // One side must be typeof, the other "undefined" string.
        let (typeof_arg, has_undefined_str) = match (&bin.left, &bin.right) {
            (Expression::UnaryExpression(unary), other) | (other, Expression::UnaryExpression(unary))
                if unary.operator == UnaryOperator::Typeof =>
            {
                let is_undef = is_undefined_string(other);
                (Some(&unary.argument), is_undef)
            }
            _ => (None, false),
        };

        let Some(arg) = typeof_arg else { return };
        if !has_undefined_str {
            return;
        }

        // SSR-detection idiom: `typeof globalThis.window === 'undefined'`,
        // `typeof window === 'undefined'`, etc. TypeScript's lib.dom types
        // these globals as never-undefined, so the `=== undefined` rewrite
        // collides with `no-unnecessary-condition`. Keep `typeof` for SSR
        // guards.
        if is_dom_global_ssr_check(arg) {
            return;
        }

        // Only flag when the operand is guaranteed to be a declared binding.
        let safe_to_rewrite = matches!(
            arg,
            Expression::StaticMemberExpression(_) | Expression::ComputedMemberExpression(_)
        );
        if !safe_to_rewrite {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer `=== undefined` over `typeof … === 'undefined'` when \
                      the operand is a property access (which cannot throw \
                      ReferenceError)."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_undefined_string(expr: &Expression) -> bool {
    match expr {
        Expression::StringLiteral(lit) => lit.value == "undefined",
        _ => false,
    }
}

/// Returns true when the typeof operand targets a DOM/browser global, either
/// directly (`window`) or via a global proxy (`globalThis.window`,
/// `self.document`, ...).
fn is_dom_global_ssr_check(arg: &Expression) -> bool {
    match arg {
        Expression::Identifier(id) => DOM_GLOBALS.contains(&id.name.as_str()),
        Expression::StaticMemberExpression(m) => {
            let prop = m.property.name.as_str();
            if !DOM_GLOBALS.contains(&prop) {
                return false;
            }
            match &m.object {
                Expression::Identifier(obj) => GLOBAL_PROXIES.contains(&obj.name.as_str()),
                _ => false,
            }
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_typeof_member_expression() {
        let d = run_on("if (typeof obj.foo === 'undefined') {}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_typeof_subscript_expression() {
        let d = run_on("if (typeof arr[0] === 'undefined') {}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_typeof_bare_identifier() {
        let d = run_on("if (typeof x === 'undefined') {}");
        assert!(d.is_empty());
    }

    // Regression for #209 — SSR guards on DOM globals must not fire.
    #[test]
    fn allows_typeof_globalthis_window() {
        let d = run_on("if (typeof globalThis.window === 'undefined') {}");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn allows_typeof_bare_window() {
        let d = run_on("if (typeof window === 'undefined') {}");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn allows_typeof_bare_document() {
        let d = run_on("if (typeof document === 'undefined') {}");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn allows_typeof_navigator_negated() {
        let d = run_on("if (typeof navigator !== 'undefined') {}");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn allows_typeof_globalthis_document() {
        let d = run_on("if (typeof globalThis.document === 'undefined') {}");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn allows_typeof_self_window() {
        let d = run_on("if (typeof self.window === 'undefined') {}");
        assert!(d.is_empty(), "{d:?}");
    }

    // Negative — non-DOM-global property accesses still flag.
    #[test]
    fn flags_typeof_non_dom_property_access() {
        let d = run_on("if (typeof someObj.someProp === 'undefined') {}");
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn flags_typeof_globalthis_non_dom() {
        let d = run_on("if (typeof globalThis.myCustomGlobal === 'undefined') {}");
        assert_eq!(d.len(), 1, "{d:?}");
    }
}
