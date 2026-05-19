//! no-side-effects-in-initialization OxcCheck backend — flag module-level
//! expression statements whose expression is a call or `new` expression.
//!
//! Exemptions:
//! - test files (path heuristic);
//! - Vitest setup files (convention path `test-helpers/setup-*.{ts,tsx,js,jsx}`,
//!   or content shape where every top-level call is a Vitest hook with a
//!   `"vitest"` import present);
//! - framework entry points reported by `is_framework_entry_point`;
//! - TanStack Start entry files (`app/{client,router,server}.{ts,tsx}` or
//!   `src/app/…`) when the `tanstack-router` framework is detected;
//! - `startTransition(...)` calls whose callee resolves to an import from
//!   `"react"` (React 18 top-level hydration pattern).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use crate::rules::path_utils::is_framework_entry_point;
use oxc_ast::ast::{
    Expression, ImportDeclarationSpecifier, Program, Statement,
};
use std::collections::HashSet;
use std::sync::Arc;

pub struct Check;

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    [".test.", ".test-d.", ".spec.", "__tests__", "_test.", ".e2e."]
        .iter()
        .any(|m| s.contains(m))
}

fn is_vitest_setup_path(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy().replace('\\', "/");
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if !(name.starts_with("setup-") || name == "setup.ts" || name == "setup.tsx"
        || name == "setup.js" || name == "setup.jsx")
    {
        return false;
    }
    s.contains("/test-helpers/") || s.starts_with("test-helpers/")
}

const VITEST_HOOK_IDENTS: &[&str] =
    &["beforeAll", "beforeEach", "afterEach", "afterAll"];

fn call_callee_text<'a>(call: &'a oxc_ast::ast::CallExpression) -> Option<&'a str> {
    match &call.callee {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(m) => {
            let Expression::Identifier(obj) = &m.object else {
                return None;
            };
            if obj.name == "expect" && m.property.name == "extend" {
                Some("expect.extend")
            } else {
                None
            }
        }
        _ => None,
    }
}

fn is_vitest_hook_call(call: &oxc_ast::ast::CallExpression) -> bool {
    match call_callee_text(call) {
        Some(name) => {
            name == "expect.extend" || VITEST_HOOK_IDENTS.contains(&name)
        }
        None => false,
    }
}

/// True when at least one `ImportDeclaration` in the program imports from
/// `"vitest"` or a `"vitest/"` sub-path.
fn has_vitest_import(program: &Program) -> bool {
    program.body.iter().any(|stmt| {
        let Statement::ImportDeclaration(import) = stmt else { return false };
        let src = import.source.value.as_str();
        src == "vitest" || src.starts_with("vitest/")
    })
}

/// True when the program has at least one top-level call/`new` expression
/// statement AND every such statement is a Vitest hook call, AND the file
/// imports from `"vitest"` (or a sub-path). An empty program (no top-level
/// expression statements) returns `false` — there's nothing to exempt.
fn shape_is_vitest_setup(program: &Program) -> bool {
    if !has_vitest_import(program) {
        return false;
    }
    let mut seen_any = false;
    for stmt in &program.body {
        let Statement::ExpressionStatement(es) = stmt else { continue };
        match &es.expression {
            Expression::CallExpression(call) => {
                seen_any = true;
                if !is_vitest_hook_call(call) {
                    return false;
                }
            }
            Expression::NewExpression(_) => return false,
            _ => {}
        }
    }
    seen_any
}

/// Collect local identifier names that are bound to `startTransition`
/// imported from `"react"`. Handles `import { startTransition } from "react"`
/// and `import { startTransition as ST } from "react"`.
fn react_start_transition_bindings(program: &Program) -> HashSet<String> {
    let mut out = HashSet::new();
    for stmt in &program.body {
        let Statement::ImportDeclaration(import) = stmt else { continue };
        if import.source.value.as_str() != "react" {
            continue;
        }
        let Some(specifiers) = &import.specifiers else { continue };
        for spec in specifiers {
            let ImportDeclarationSpecifier::ImportSpecifier(named) = spec else {
                continue;
            };
            if named.imported.name() == "startTransition" {
                out.insert(named.local.name.to_string());
            }
        }
    }
    out
}

fn is_start_transition_call(
    call: &oxc_ast::ast::CallExpression,
    bindings: &HashSet<String>,
) -> bool {
    let Expression::Identifier(id) = &call.callee else { return false };
    bindings.contains(id.name.as_str())
}

/// True when `path` is a TanStack Start entry file: `app/client.{ts,tsx}`,
/// `app/router.{ts,tsx}`, or `app/server.{ts,tsx}` (also under `src/app/`).
/// Requires the project to have the `tanstack-router` framework detected.
fn is_tanstack_start_entry(path: &std::path::Path, project: &crate::project::ProjectCtx) -> bool {
    if !project.has_framework("tanstack-router") {
        return false;
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let stem = if let Some(s) = name.strip_suffix(".tsx") {
        s
    } else if let Some(s) = name.strip_suffix(".ts") {
        s
    } else {
        return false;
    };
    if !matches!(stem, "client" | "router" | "server") {
        return false;
    }
    let s = path.to_string_lossy().replace('\\', "/");
    s.contains("/app/client.") || s.contains("/app/router.") || s.contains("/app/server.")
        || s == "app/client.ts" || s == "app/client.tsx"
        || s == "app/router.ts" || s == "app/router.tsx"
        || s == "app/server.ts" || s == "app/server.tsx"
}

fn effectful_expression_label(expr: &Expression) -> Option<&'static str> {
    match expr {
        Expression::CallExpression(_) => Some("call"),
        Expression::NewExpression(_) => Some("`new` expression"),
        _ => None,
    }
}

fn has_pure_annotation(source: &str, span_start: usize) -> bool {
    // Look backwards from the statement start for a PURE comment.
    let before = &source[..span_start];
    let trimmed = before.trim_end();
    trimmed.ends_with("*/")
        && (trimmed.contains("#__PURE__") || trimmed.contains("@__PURE__"))
        && {
            // The comment must be the immediately preceding token.
            if let Some(comment_start) = trimmed.rfind("/*") {
                let comment = &trimmed[comment_start..];
                comment.contains("#__PURE__") || comment.contains("@__PURE__")
            } else {
                false
            }
        }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if ctx.file.path_segments.in_test_dir || is_test_file(ctx.path) {
            return Vec::new();
        }

        let program = semantic.nodes().program();

        if is_vitest_setup_path(ctx.path) || shape_is_vitest_setup(program) {
            return Vec::new();
        }

        if is_framework_entry_point(ctx.path, ctx.project)
            || is_tanstack_start_entry(ctx.path, ctx.project)
        {
            return Vec::new();
        }

        let start_transition_names = react_start_transition_bindings(program);

        let mut diagnostics = Vec::new();
        for stmt in &program.body {
            let Statement::ExpressionStatement(expr_stmt) = stmt else { continue };
            let Some(label) = effectful_expression_label(&expr_stmt.expression) else {
                continue;
            };

            if let Expression::CallExpression(call) = &expr_stmt.expression
                && is_start_transition_call(call, &start_transition_names)
            {
                continue;
            }

            if has_pure_annotation(ctx.source, expr_stmt.span.start as usize) {
                continue;
            }

            let (line, column) =
                byte_offset_to_line_col(ctx.source, expr_stmt.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Top-level {label} executes on import and blocks tree-shaking. \
                     Move it into a function, or mark it `/*#__PURE__*/` if truly side-effect-free."
                ),
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
    use crate::rules::test_helpers::{
        run_oxc_ts, run_oxc_ts_with_path, run_oxc_tsx_with_path_and_framework,
    };

    #[test]
    fn flags_top_level_bare_call() {
        let diags = run_oxc_ts("doThing();", &Check);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_top_level_new_expression() {
        let diags = run_oxc_ts("new EventEmitter();", &Check);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_pure_annotated_call() {
        let diags = run_oxc_ts("/*#__PURE__*/ registerSomething();", &Check);
        assert!(diags.is_empty());
    }

    #[test]
    fn skips_test_files() {
        let diags = run_oxc_ts_with_path(
            "expectType<string>(foo());",
            &Check,
            "main.test-d.ts",
        );
        assert!(diags.is_empty());
    }

    // --- (a) Vitest setup file exemption ----------------------------------

    #[test]
    fn allows_vitest_setup_file_by_convention_path() {
        let src = "\
            import { beforeAll, afterEach } from 'vitest';\n\
            beforeAll(() => { startMockServer({ onUnhandledRequest: 'error' }); });\n\
            afterEach(() => { mswServer.resetHandlers(); });\n";
        let diags = run_oxc_ts_with_path(
            src,
            &Check,
            "src/test-helpers/setup-msw.ts",
        );
        assert!(
            diags.is_empty(),
            "vitest setup file by convention path should be exempt, got {diags:?}"
        );
    }

    #[test]
    fn allows_vitest_setup_file_by_content_shape() {
        let src = "\
            import { beforeAll, afterEach } from 'vitest';\n\
            beforeAll(() => { boot(); });\n\
            afterEach(() => { reset(); });\n\
            expect.extend({ toBeFoo() { return { pass: true, message: () => '' }; } });\n";
        // Path does NOT match the convention — content shape carries the exemption.
        let diags = run_oxc_ts_with_path(src, &Check, "src/some/random/file.ts");
        assert!(
            diags.is_empty(),
            "all-hooks content shape should exempt the file, got {diags:?}"
        );
    }

    #[test]
    fn flags_top_level_beforeAll_without_vitest_import() {
        // `beforeAll` defined locally — no vitest import — shape check must NOT exempt.
        let src = "\
            function beforeAll(fn: () => void) { fn(); }\n\
            beforeAll(() => someSideEffect());\n";
        let diags = run_oxc_ts_with_path(src, &Check, "src/foo.ts");
        assert_eq!(
            diags.len(),
            1,
            "beforeAll without vitest import must be flagged, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_when_content_mixes_hooks_with_other_calls() {
        let src = "\
            beforeAll(() => { boot(); });\n\
            someOtherCall();\n";
        let diags = run_oxc_ts_with_path(src, &Check, "src/some/file.ts");
        assert_eq!(
            diags.len(),
            2,
            "non-hook call breaks the setup-file shape, both stmts flagged"
        );
    }

    // --- (b) Framework entry point exemption ------------------------------

    #[test]
    fn allows_tanstack_start_client_entry() {
        // `client.tsx` at any depth is a TanStack Start entry point.
        let src = "\
            import { startTransition } from 'react';\n\
            import { hydrateRoot } from 'react-dom/client';\n\
            initZodLocale();\n\
            stripSensitiveQueryFromUrlBar();\n\
            startTransition(() => { hydrateRoot(document, <StartClient />); });\n";
        let diags = run_oxc_tsx_with_path_and_framework(
            src,
            &Check,
            "src/app/client.tsx",
            "tanstack-router",
        );
        assert!(
            diags.is_empty(),
            "framework entry point should be exempt, got {diags:?}"
        );
    }

    #[test]
    fn allows_tanstack_router_entry() {
        let src = "createRouter({ routeTree, defaultPreload: 'intent' });\n";
        let diags = run_oxc_tsx_with_path_and_framework(
            src,
            &Check,
            "src/app/router.tsx",
            "tanstack-router",
        );
        assert!(diags.is_empty(), "router.tsx entry should be exempt");
    }

    #[test]
    fn flags_client_tsx_outside_app_dir() {
        // Same pattern as the entry — but the file lives outside app/, so NOT exempt.
        let src = "\
            import { startTransition } from 'react';\n\
            import { hydrateRoot } from 'react-dom/client';\n\
            initZodLocale();\n";
        let diags = run_oxc_tsx_with_path_and_framework(
            src,
            &Check,
            "src/utils/client.tsx",
            "tanstack-router",
        );
        assert_eq!(
            diags.len(),
            1,
            "client.tsx outside app/ must still be flagged, got {diags:?}"
        );
    }

    // --- (c) `startTransition` from "react" -------------------------------

    #[test]
    fn allows_start_transition_from_react() {
        let src = "\
            import { startTransition } from 'react';\n\
            startTransition(() => { hydrateRoot(document, null); });\n";
        let diags = run_oxc_ts(src, &Check);
        assert!(
            diags.is_empty(),
            "startTransition imported from react is exempt, got {diags:?}"
        );
    }

    #[test]
    fn allows_aliased_start_transition_from_react() {
        let src = "\
            import { startTransition as ST } from 'react';\n\
            ST(() => { hydrateRoot(document, null); });\n";
        let diags = run_oxc_ts(src, &Check);
        assert!(
            diags.is_empty(),
            "aliased startTransition import is exempt, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_start_transition_from_other_source() {
        let src = "\
            import { startTransition } from 'some-other-lib';\n\
            startTransition(() => { boot(); });\n";
        let diags = run_oxc_ts(src, &Check);
        assert_eq!(
            diags.len(),
            1,
            "startTransition not from react is still flagged"
        );
    }

    #[test]
    fn still_flags_bare_start_transition_identifier_without_import() {
        let src = "startTransition(() => { boot(); });\n";
        let diags = run_oxc_ts(src, &Check);
        assert_eq!(
            diags.len(),
            1,
            "no import binding means no exemption"
        );
    }

}
