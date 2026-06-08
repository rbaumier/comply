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
//!
//! Variables assigned from `Promise.withResolvers()` are also exempt: their
//! `.resolve` property is the deferred-promise resolver function, not an Elysia
//! chain method.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::collections::HashSet;
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

/// Collect the names of all variables assigned from `Promise.withResolvers()`.
///
/// Matches `const/let/var x = Promise.withResolvers()` (with or without type
/// arguments). Their `.resolve` property is the deferred-promise resolver, not
/// an Elysia chain method.
fn collect_with_resolvers_var_names(semantic: &oxc_semantic::Semantic<'_>) -> HashSet<String> {
    let mut names = HashSet::new();
    for node in semantic.nodes().iter() {
        let oxc_ast::AstKind::VariableDeclarator(decl) = node.kind() else {
            continue;
        };
        let oxc_ast::ast::BindingPattern::BindingIdentifier(ident) = &decl.id else {
            continue;
        };
        let Some(init) = &decl.init else { continue };
        let Expression::CallExpression(call) = init else { continue };
        let Expression::StaticMemberExpression(member) = &call.callee else { continue };
        if member.property.name.as_str() != "withResolvers" {
            continue;
        }
        if !is_promise_receiver(&member.object) {
            continue;
        }
        names.insert(ident.name.as_str().to_owned());
    }
    names
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

/// Return true when any `.as('scoped')` call expression exists in the program.
///
/// `.as('scoped')` restricts a plugin's derived values to routes defined after
/// its `.use()` — semantically equivalent to `.guard({ resolve })` for scope
/// restriction, so `.resolve()` in such a chain does not leak. `.as('global')`
/// is deliberately NOT exempt: it propagates everywhere, which is the leak the
/// rule warns about.
fn file_has_as_scoped_call(semantic: &oxc_semantic::Semantic<'_>) -> bool {
    for node in semantic.nodes().iter() {
        let oxc_ast::AstKind::CallExpression(call) = node.kind() else {
            continue;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            continue;
        };
        if member.property.name.as_str() != "as" {
            continue;
        }
        if let Some(Expression::StringLiteral(s)) =
            call.arguments.first().and_then(|a| a.as_expression())
            && s.value.as_str() == "scoped"
        {
            return true;
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
        if file_has_as_scoped_call(semantic) {
            return Vec::new();
        }

        let with_resolvers_vars = collect_with_resolvers_var_names(semantic);
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

            // Skip `.resolve()` on variables assigned from `Promise.withResolvers()`.
            if let Expression::Identifier(ident) = &member.object {
                if with_resolvers_vars.contains(ident.name.as_str()) {
                    continue;
                }
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
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.ts", &crate::project::ProjectCtx::for_test_with_framework("elysia"), crate::rules::file_ctx::default_static_file_ctx())
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

    // Regression for #235: a no-op `await Promise.resolve()` inside an async
    // object-method (needed to satisfy promise-function-async on a sync body)
    // is the JS built-in, not an Elysia `.resolve()`.
    #[test]
    fn ignores_promise_resolve_in_async_object_method() {
        let src = r#"
            const sender = {
                send: async (email) => {
                    await Promise.resolve();
                    return Result.ok();
                },
            };
        "#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
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

    // Regression for #386: `Promise.resolve()` inside a vi.fn() async callback
    // (get-session.test.ts pattern) must not be flagged.
    #[test]
    fn ignores_promise_resolve_in_vi_fn_callback() {
        let src = r#"
            import { vi } from "vitest";
            function makeAuth(implementation) {
                const getSession = vi.fn(async (input) => {
                    await Promise.resolve();
                    return implementation(input);
                });
                return { auth: { api: { getSession } }, getSession };
            }
        "#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // Regression for #386: `Promise.resolve()` inside an async method of an
    // object returned from a function (authorization.test.ts pattern).
    #[test]
    fn ignores_promise_resolve_in_returned_object_method() {
        let src = r#"
            import { Elysia } from "elysia";
            function makeAuthReturningNoSession() {
                return {
                    api: {
                        getSession: async () => {
                            await Promise.resolve();
                            return null;
                        },
                    },
                };
            }
            async function run() {
                const app = new Elysia().get("/protected", () => ({ ok: true }));
            }
        "#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // Regression for #386: `.resolve()` on the result of `Promise.withResolvers()`
    // is the JS standard deferred-promise API, not Elysia's `.resolve()`.
    #[test]
    fn ignores_with_resolvers_resolve() {
        let src = r#"
            const gate = Promise.withResolvers();
            gate.resolve(true);
        "#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // Regression for #533: `.resolve()` followed by `.as('scoped')` restricts the
    // derived value's scope — equivalent to `.guard({ resolve })`.
    #[test]
    fn allows_resolve_with_as_scoped() {
        let src = r#"
            import { Elysia } from 'elysia';
            return new Elysia({ name: 'require-authorization' })
                .use(requestIdPlugin)
                .resolve(async ({ request, requestId }) => ({ user: request }))
                .as('scoped');
        "#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_resolve_with_as_scoped_double_quotes() {
        let src = r#"
            import { Elysia } from 'elysia';
            new Elysia().resolve(({ headers }) => ({ user: headers.x })).as("scoped");
        "#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn still_flags_resolve_with_as_global() {
        // `.as('global')` propagates everywhere — the leak the rule warns about.
        let src = r#"
            import { Elysia } from 'elysia';
            new Elysia().resolve(({ headers }) => ({ user: headers.x })).as('global');
        "#;
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    // Regression for #386: destructured `.resolve` from `Promise.withResolvers()`.
    #[test]
    fn ignores_destructured_with_resolvers_resolve() {
        let src = r#"
            const { promise, resolve, reject } = Promise.withResolvers();
            resolve("done");
        "#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }
}
