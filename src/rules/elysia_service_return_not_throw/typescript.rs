//! elysia-service-return-not-throw backend — flag `throw` in elysia service files.
//!
//! Elysia projects often share a monorepo with frontend code (React,
//! TanStack Router, Vue, Svelte). `throw redirect()` and `throw` inside a
//! React context provider are legitimate frontend patterns. The rule scans
//! the file's import statements and only proceeds when there's an actual
//! Elysia import (`elysia` or `@elysiajs/...`), and skips files that
//! import frontend frameworks.

use crate::diagnostic::{Diagnostic, Severity};

fn imports_elysia(source: &str) -> bool {
    source.contains("from 'elysia'")
        || source.contains("from \"elysia\"")
        || source.contains("from 'elysia/")
        || source.contains("from \"elysia/")
        || source.contains("from '@elysiajs/")
        || source.contains("from \"@elysiajs/")
}

fn imports_frontend(source: &str) -> bool {
    source.contains("from 'react'")
        || source.contains("from \"react\"")
        || source.contains("from 'react/")
        || source.contains("from \"react/")
        || source.contains("from 'react-dom")
        || source.contains("from \"react-dom")
        || source.contains("from '@tanstack/")
        || source.contains("from \"@tanstack/")
        || source.contains("from 'vue'")
        || source.contains("from \"vue\"")
        || source.contains("from 'svelte")
        || source.contains("from \"svelte")
        || source.contains("from 'solid-js")
        || source.contains("from \"solid-js")
}

crate::ast_check! { on ["throw_statement"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }
    if !imports_elysia(ctx.source) {
        return;
    }
    if imports_frontend(ctx.source) {
        return;
    }

    let _ = source;
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-service-return-not-throw".into(),
        message: "`throw` in Elysia code breaks typed error propagation — return `status(code, message)` instead.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_throw_new_error() {
        let src = "import { Elysia } from 'elysia';\nfunction svc() { throw new Error('boom'); }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_throw_string() {
        let src = "import { Elysia } from 'elysia';\nfunction svc() { throw 'no'; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_status_return() {
        let src = "import { Elysia, status } from 'elysia';\nfunction svc() { return status(404, 'not found'); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "function svc() { throw new Error('boom'); }";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }

    #[test]
    fn ignores_react_context_provider() {
        // Regression: React context providers `throw` to detect missing
        // providers. Even in a project that has Elysia somewhere, files that
        // import React are not Elysia services.
        let src = "import { createContext, useContext } from 'react';\nconst Ctx = createContext(null);\nexport function useCtx() { const v = useContext(Ctx); if (!v) throw new Error('missing provider'); return v; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_tanstack_route_loader() {
        // Regression: TanStack Router uses `throw redirect(...)` in loaders.
        let src = "import { redirect } from '@tanstack/react-router';\nexport const loader = () => { throw redirect({ to: '/login' }); };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_file_without_elysia_import() {
        // Plain backend util that doesn't import Elysia — leave it alone.
        let src = "export function parse(x: string) { if (!x) throw new Error('empty'); return JSON.parse(x); }";
        assert!(run_on(src).is_empty());
    }
}
