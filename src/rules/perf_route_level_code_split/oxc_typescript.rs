//! perf-route-level-code-split — OXC backend.
//! Flag static `import Foo from './pages/...'` (or routes/views/) patterns.

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::Language;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// The remediation is `React.lazy(() => import(...))`, so the rule only applies
/// to React code: `.tsx`/`.jsx` files (JSX implies React) or a `.ts`/`.js`
/// module that imports React. Vue Router (and other frameworks) use a `views/`
/// convention too but split routes via `component: () => import(...)`, so they
/// are out of scope.
fn in_react_context(ctx: &CheckCtx) -> bool {
    matches!(ctx.lang, Language::Tsx) || imports_react(ctx.source)
}

fn imports_react(source: &str) -> bool {
    use crate::oxc_helpers::source_contains;
    source_contains(source, "from \"react\"")
        || source_contains(source, "from 'react'")
        || source_contains(source, "from \"react/")
        || source_contains(source, "from 'react/")
        || source_contains(source, "require(\"react\")")
        || source_contains(source, "require('react')")
}

fn looks_like_route_module(spec: &str) -> bool {
    spec.contains("/pages/")
        || spec.contains("/routes/")
        || spec.contains("/views/")
        || spec.starts_with("./pages/")
        || spec.starts_with("./routes/")
        || spec.starts_with("./views/")
        || spec.starts_with("../pages/")
        || spec.starts_with("../routes/")
        || spec.starts_with("../views/")
}

/// React Router v7 / Remix `*.server.*` modules are server-only: the bundler
/// strips them from the client bundle, they hold no JSX, and they cannot be
/// `React.lazy()`-loaded — so the code-split advice never applies, even when
/// colocated under `/routes/`. The `.server.` suffix is matched on the path
/// portion (a trailing `?raw`-style query is stripped); the leading `#` of a
/// Node.js subpath import (`#app/...`) is preserved.
fn is_server_only_module(spec: &str) -> bool {
    let path = spec.split_once('?').map_or(spec, |(head, _)| head);
    [".server.ts", ".server.tsx", ".server.js", ".server.jsx", ".server.mts"]
        .iter()
        .any(|suffix| path.ends_with(suffix))
}

fn is_test_or_e2e(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains("/e2e/")
        || s.contains("__tests__")
        || s.contains(".test.")
        || s.contains(".spec.")
        || s.contains(".stories.")
        || s.contains("/test/")
        || s.contains("/tests/")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["/pages/", "/routes/", "/views/"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ImportDeclaration(import) = node.kind() else { return };

        if !in_react_context(ctx) {
            return;
        }

        if is_test_or_e2e(ctx.path) {
            return;
        }

        // Skip `import type ...`.
        if import.import_kind.is_type() {
            return;
        }

        let spec = import.source.value.as_str();
        if !looks_like_route_module(spec) {
            return;
        }

        // Server-only `*.server.*` modules cannot be lazy-loaded — exempt them.
        if is_server_only_module(spec) {
            return;
        }

        // Must have a binding (default, named, or namespace import).
        let has_binding = import.specifiers.as_ref().is_some_and(|s| !s.is_empty());
        if !has_binding {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Static import of route module '{spec}' — wrap it in `React.lazy(() => import('{spec}'))` so the route bundle is split."
            ),
            severity: Severity::Warning,
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

    fn run(s: &str) -> Vec<Diagnostic> {
        // `.tsx` => JSX implies React, so the React-context gate is satisfied.
        crate::rules::test_helpers::run_rule(&Check, s, "t.tsx")
    }

    #[test]
    fn flags_static_pages_import() {
        assert_eq!(run("import Home from './pages/Home';").len(), 1);
    }

    #[test]
    fn flags_routes_subdir_import() {
        assert_eq!(run("import Settings from '../routes/Settings';").len(), 1);
    }

    #[test]
    fn flags_views_import() {
        assert_eq!(run("import Dashboard from 'src/views/Dashboard';").len(), 1);
    }

    #[test]
    fn flags_route_import_in_ts_file_that_imports_react() {
        let d = crate::rules::test_helpers::run_rule(
            &Check,
            "import React from 'react';\nimport Home from './pages/Home';",
            "t.ts",
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_type_only_route_import() {
        assert!(run("import type { Props } from './pages/Home';").is_empty());
    }

    #[test]
    fn allows_non_route_import() {
        assert!(run("import { clsx } from 'clsx';").is_empty());
    }

    #[test]
    fn allows_server_only_module_under_routes() {
        // RR v7 / Remix server-only module colocated under /routes/ — stripped
        // from the client bundle, cannot be React.lazy()-loaded.
        assert!(
            run("import { requireRecentVerification } from '#app/routes/_auth/verify.server.ts';")
                .is_empty()
        );
        assert!(
            run("import { x } from '#app/routes/_auth/verify.server.tsx';").is_empty()
        );
    }

    #[test]
    fn still_flags_route_component_without_server_suffix() {
        // A real route component (no `.server.` suffix) must still be flagged.
        assert_eq!(run("import Foo from '#app/routes/dashboard/route.tsx';").len(), 1);
    }

    #[test]
    fn skips_vue_router_views_imports() {
        // Vue Router playground router.ts: static `./views/*.vue` imports with no
        // React anywhere — splits routes via `component: () => import(...)`, not
        // `React.lazy`, so the rule must not fire.
        let src = "import Home from './views/Home.vue'\n\
                   import Nested from './views/Nested.vue'\n\
                   import NestedWithId from './views/NestedWithId.vue'\n\
                   import Dynamic from './views/Dynamic.vue'\n\
                   import User from './views/User.vue'\n";
        let d = crate::rules::test_helpers::run_rule(&Check, src, "router.ts");
        assert!(d.is_empty());
    }

    #[test]
    fn skips_e2e_files() {
        let d = crate::rules::test_helpers::run_rule(&Check, "import LoginPage from './pages/login.page';", "project/e2e/fixtures.ts");
        assert!(d.is_empty());
    }
}
