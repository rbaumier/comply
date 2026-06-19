//! tanstack-start-no-server-import-in-isomorphic-entry oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

/// Filenames TanStack Start runs in both the client and server bundles, so a
/// static server-only import here leaks Node code into the browser. Restricted
/// to the unambiguous entry names; a generic `client.ts` is excluded because it
/// is commonly an API/DB client (`src/db/client.ts`) that legitimately imports
/// `pg`.
const ISOMORPHIC_ENTRIES: &[&str] = &["router.ts", "router.tsx", "start.ts", "start.tsx"];

fn is_isomorphic_entry(ctx: &CheckCtx) -> bool {
    let file_name = ctx.path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    ISOMORPHIC_ENTRIES.contains(&file_name)
}

/// Returns true for packages whose code only runs on a Node/Bun server.
fn is_server_only_specifier(specifier: &str) -> bool {
    specifier == "@sentry/node"
        || specifier.starts_with("@sentry/node/")
        || specifier.starts_with("@sentry/node-")
        || specifier.starts_with("node:")
        || specifier.starts_with("bun:")
        || specifier == "pg"
        || specifier.starts_with("pg/")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        // Only static, top-level `import ... from "..."` declarations. A dynamic
        // `import()` (the SSR-gated escape hatch) parses as an `ImportExpression`
        // and is never visited here, so it is skipped by construction.
        &[AstType::ImportDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_isomorphic_entry(ctx) {
            return;
        }
        let AstKind::ImportDeclaration(import) = node.kind() else {
            return;
        };
        let specifier = import.source.value.as_str();
        if !is_server_only_specifier(specifier) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{specifier}` is server-only; a static import in this isomorphic entry ships Node code into the client bundle. Gate it behind `if (import.meta.env.SSR)` with a dynamic `import()`."
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

    fn run(src: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, path)
    }

    #[test]
    fn flags_sentry_node_in_router() {
        let diags = run(r#"import * as Sentry from "@sentry/node";"#, "src/router.tsx");
        assert_eq!(diags.len(), 1, "got {diags:?}");
        assert!(diags[0].message.contains("@sentry/node"));
    }

    #[test]
    fn flags_pg_in_router() {
        let diags = run(r#"import pg from "pg";"#, "src/router.tsx");
        assert_eq!(diags.len(), 1, "got {diags:?}");
    }

    #[test]
    fn flags_node_builtin_in_start() {
        let diags = run(r#"import { readFile } from "node:fs";"#, "src/start.ts");
        assert_eq!(diags.len(), 1, "got {diags:?}");
    }

    #[test]
    fn flags_bun_builtin_in_start_tsx() {
        let diags = run(r#"import { sql } from "bun:sqlite";"#, "src/start.tsx");
        assert_eq!(diags.len(), 1, "got {diags:?}");
    }

    #[test]
    fn flags_sentry_node_core_in_router() {
        // `@sentry/node-core` is a server-only Sentry SDK; the `@sentry/node-`
        // prefix must catch it.
        let diags = run(r#"import * as Sentry from "@sentry/node-core";"#, "src/router.tsx");
        assert_eq!(diags.len(), 1, "got {diags:?}");
    }

    #[test]
    fn flags_pg_subpath_in_router() {
        // `pg/...` subpaths are part of the `pg` package and must fire.
        let diags = run(r#"import native from "pg/lib/native";"#, "src/router.tsx");
        assert_eq!(diags.len(), 1, "got {diags:?}");
    }

    #[test]
    fn no_fire_on_generic_client_module() {
        // A generic `client.ts` (e.g. a DB client) is NOT a TanStack Start
        // isomorphic entry — a server-side `pg` import here is legitimate.
        let diags = run(r#"import pg from "pg";"#, "src/db/client.ts");
        assert!(diags.is_empty(), "generic client.ts wrongly flagged: {diags:?}");
    }

    #[test]
    fn no_fire_outside_isomorphic_entry_server_file() {
        // A `.server.ts` file is covered by other rules — not an isomorphic entry.
        let diags = run(r#"import * as Sentry from "@sentry/node";"#, "src/auth.server.ts");
        assert!(diags.is_empty(), "got {diags:?}");
    }

    #[test]
    fn no_fire_in_arbitrary_file() {
        let diags = run(r#"import pg from "pg";"#, "src/utils.ts");
        assert!(diags.is_empty(), "got {diags:?}");
    }

    #[test]
    fn no_fire_on_ssr_gated_dynamic_import() {
        // The SSR-gated dynamic `import()` is an ImportExpression, not an
        // ImportDeclaration, so it is never flagged.
        let src = r#"if (import.meta.env.SSR) { const Sentry = await import("@sentry/node"); }"#;
        let diags = run(src, "src/router.tsx");
        assert!(diags.is_empty(), "got {diags:?}");
    }

    #[test]
    fn no_fire_on_non_server_import_in_router() {
        let diags = run(r#"import { z } from "zod";"#, "src/router.tsx");
        assert!(diags.is_empty(), "got {diags:?}");
    }

    #[test]
    fn no_fire_on_sentry_react_specifier_overmatch() {
        // `@sentry/react` is a client package — the prefix list must not catch it.
        let diags = run(r#"import * as Sentry from "@sentry/react";"#, "src/router.tsx");
        assert!(diags.is_empty(), "got {diags:?}");
    }

    #[test]
    fn no_fire_on_pg_promise_specifier_overmatch() {
        // `pg-promise` and `pglite` are distinct packages — `pg` must match
        // exactly or as a `pg/` subpath only.
        let diags = run(r#"import pgp from "pg-promise";"#, "src/router.tsx");
        assert!(diags.is_empty(), "pg-promise wrongly flagged: {diags:?}");
        let diags = run(r#"import { PGlite } from "pglite";"#, "src/router.tsx");
        assert!(diags.is_empty(), "pglite wrongly flagged: {diags:?}");
    }
}
