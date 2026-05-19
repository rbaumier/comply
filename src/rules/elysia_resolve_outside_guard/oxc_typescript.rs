//! elysia-resolve-outside-guard oxc backend — flag top-level `.resolve(`.
//!
//! Skip the file when an AST-level `.guard(...)` call is detected — `.resolve()`
//! inside a `.guard()` chain is the intended pattern. Detection is an AST walk,
//! not a text scan, so `.guard(` in a comment or string literal does not silence
//! the diagnostic.
//!
//! `Promise.resolve(...)` is also exempt — including the globalised forms
//! `globalThis.Promise.resolve()`, `window.Promise.resolve()`,
//! `self.Promise.resolve()`, and `this.Promise.resolve()` — since these are
//! the standard JavaScript promise constructor, not an Elysia chain.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// Return true when `expr` resolves to the global `Promise` constructor.
///
/// Matches bare `Promise` as well as the global-object access forms
/// `globalThis.Promise`, `window.Promise`, `self.Promise`, and `this.Promise`.
fn is_promise_receiver(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(ident) => ident.name.as_str() == "Promise",
        Expression::StaticMemberExpression(member) => {
            if member.property.name.as_str() != "Promise" {
                return false;
            }
            match &member.object {
                Expression::Identifier(host) => matches!(
                    host.name.as_str(),
                    "globalThis" | "window" | "self"
                ),
                Expression::ThisExpression(_) => true,
                _ => false,
            }
        }
        _ => false,
    }
}

/// Return true when any `.guard(...)` call expression exists in the program.
///
/// Walks `semantic.nodes()` looking for a `CallExpression` whose callee is a
/// `StaticMemberExpression` with property name `"guard"`. Comments and string
/// literals that happen to contain `.guard(` do not satisfy this.
fn file_has_guard_call(semantic: &oxc_semantic::Semantic<'_>) -> bool {
    for node in semantic.nodes().iter() {
        let oxc_ast::AstKind::CallExpression(call) = node.kind() else {
            continue;
        };
        if let Expression::StaticMemberExpression(member) = &call.callee {
            if member.property.name.as_str() == "guard" {
                return true;
            }
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".resolve"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !ctx.project.has_framework("elysia") {
            return Vec::new();
        }
        if file_has_guard_call(semantic) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            let oxc_ast::AstKind::CallExpression(call) = node.kind() else {
                continue;
            };
            let Expression::StaticMemberExpression(member) = &call.callee else {
                continue;
            };
            if member.property.name.as_str() != "resolve" {
                continue;
            }

            // Skip global `Promise.resolve(...)` in all its forms.
            if is_promise_receiver(&member.object) {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`.resolve()` is used outside `.guard()` — derived values leak to every route in the chain.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_oxc_ts_with_framework;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        run_oxc_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_top_level_resolve_on_new_elysia() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().resolve(({ headers }) => ({ user: headers.x }));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_top_level_resolve_on_app() {
        let src = "import { Elysia } from 'elysia';\napp.resolve(({ headers }) => ({ user: headers.x }));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_resolve_inside_guard() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().guard({}, app => app.resolve(({ headers }) => ({ user: headers.x })));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_promise_resolve_static() {
        let src = "async function makeAsync(): Promise<number> {\n  await Promise.resolve();\n  return 42;\n}";
        assert!(run_on(src).is_empty(), "Promise.resolve() must not be flagged");
    }

    #[test]
    fn ignores_promise_resolve_with_arg() {
        let src = "await Promise.resolve(42);";
        assert!(run_on(src).is_empty(), "Promise.resolve(42) must not be flagged");
    }

    // ── Fix 1: globalThis.Promise / window.Promise / self.Promise / this.Promise ──

    #[test]
    fn ignores_globalthis_promise_resolve() {
        let src = "await globalThis.Promise.resolve(42);";
        assert!(
            run_on(src).is_empty(),
            "globalThis.Promise.resolve() must not be flagged"
        );
    }

    #[test]
    fn ignores_window_promise_resolve() {
        let src = "await window.Promise.resolve(42);";
        assert!(
            run_on(src).is_empty(),
            "window.Promise.resolve() must not be flagged"
        );
    }

    #[test]
    fn ignores_self_promise_resolve() {
        let src = "await self.Promise.resolve(42);";
        assert!(
            run_on(src).is_empty(),
            "self.Promise.resolve() must not be flagged"
        );
    }

    #[test]
    fn ignores_this_promise_resolve() {
        let src = "class C { m() { return this.Promise.resolve(42); } }";
        assert!(
            run_on(src).is_empty(),
            "this.Promise.resolve() must not be flagged"
        );
    }

    #[test]
    fn flags_non_global_promise_resolve() {
        // `obj.Promise` where `obj` is not globalThis/window/self/this — still flagged.
        let src = "import { Elysia } from 'elysia';\nobj.Promise.resolve(42);";
        assert_eq!(
            run_on(src).len(),
            1,
            "obj.Promise.resolve() (non-global host) should still flag"
        );
    }

    // ── Fix 2: AST-based `.guard(` detection, not text scan ──

    #[test]
    fn guard_as_real_call_exempts_resolve() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().guard({}, app => app);\napp.resolve(({ headers }) => ({ user: headers.x }));";
        assert!(
            run_on(src).is_empty(),
            "real .guard() call should exempt .resolve() in same file"
        );
    }

    #[test]
    fn guard_inside_string_literal_does_not_exempt() {
        let src = "import { Elysia } from 'elysia';\nconst doc = \"see app.guard(opts) docs\";\napp.resolve(({ headers }) => ({ user: headers.x }));";
        assert_eq!(
            run_on(src).len(),
            1,
            ".guard( inside a string literal must not exempt .resolve()"
        );
    }

    #[test]
    fn guard_inside_comment_does_not_exempt() {
        let src = "import { Elysia } from 'elysia';\n// example: app.guard({}, a => a)\napp.resolve(({ headers }) => ({ user: headers.x }));";
        assert_eq!(
            run_on(src).len(),
            1,
            ".guard( inside a comment must not exempt .resolve()"
        );
    }
}
