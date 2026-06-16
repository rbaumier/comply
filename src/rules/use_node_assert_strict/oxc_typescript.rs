//! use-node-assert-strict oxc backend.
//!
//! Mirrors Biome `useNodeAssertStrict`: importing the loose `node:assert`
//! module is flagged in favour of the stricter `node:assert/strict`. Biome
//! queries `AnyJsImportLike` (a static module source, a `require(...)` call, or
//! a dynamic `import(...)`) and fires when the specifier is exactly the string
//! `node:assert`. `node:assert/strict` and the bare `assert` specifier are left
//! alone — only the exact `node:` form is promoted.

use std::sync::Arc;

use oxc_ast::AstKind;
use oxc_ast::ast::{Argument, Expression};

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};

pub struct Check;

/// The single loose specifier Biome promotes. Exact match only: `node:assert`.
/// `node:assert/strict` (already strict) and bare `assert` (ambiguous, may be a
/// userland module) are intentionally not flagged.
const NODE_ASSERT: &str = "node:assert";

fn emit(ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>, offset: usize) {
    let (line, column) = byte_offset_to_line_col(ctx.source, offset);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: "Use `node:assert/strict` instead of `node:assert`. The use of stricter assertion is preferred."
            .to_string(),
        severity: Severity::Warning,
        span: None,
    });
}

impl OxcCheck for Check {
    // Empty: this rule inspects three distinct node kinds (static import/export
    // sources, `require` call expressions, dynamic import expressions) in one
    // pass. The dispatcher only calls `run_on_semantic` when `interested_kinds`
    // is empty, so all work lives there.
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        // Every firing path requires the exact `node:assert` substring in the
        // source. Files without it are skipped before parsing.
        Some(&[NODE_ASSERT])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            match node.kind() {
                // `import … from 'node:assert'` (default / named / namespace /
                // side-effect — Biome reads the `JsModuleSource` token in all
                // cases).
                AstKind::ImportDeclaration(import) => {
                    if import.source.value.as_str() == NODE_ASSERT {
                        emit(ctx, &mut diagnostics, import.source.span.start as usize);
                    }
                }
                // `export … from 'node:assert'` — the re-export source is also a
                // `JsModuleSource`, so Biome flags it too.
                AstKind::ExportNamedDeclaration(export) => {
                    if let Some(source) = &export.source
                        && source.value.as_str() == NODE_ASSERT
                    {
                        emit(ctx, &mut diagnostics, source.span.start as usize);
                    }
                }
                // `export * from 'node:assert'`.
                AstKind::ExportAllDeclaration(export) => {
                    if export.source.value.as_str() == NODE_ASSERT {
                        emit(ctx, &mut diagnostics, export.source.span.start as usize);
                    }
                }
                // Dynamic `import('node:assert')`.
                AstKind::ImportExpression(import) => {
                    if let Expression::StringLiteral(s) = &import.source
                        && s.value.as_str() == NODE_ASSERT
                    {
                        emit(ctx, &mut diagnostics, s.span.start as usize);
                    }
                }
                // `require('node:assert')`.
                AstKind::CallExpression(call) => {
                    let is_require = matches!(
                        &call.callee,
                        Expression::Identifier(id) if id.name.as_str() == "require"
                    );
                    if !is_require {
                        continue;
                    }
                    // Biome only reads a `JsStringLiteralExpression` argument;
                    // a template literal or computed argument is out of scope.
                    if let Some(Argument::StringLiteral(s)) = call.arguments.first()
                        && s.value.as_str() == NODE_ASSERT
                    {
                        emit(ctx, &mut diagnostics, call.span.start as usize);
                    }
                }
                _ => {}
            }
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

    fn run_on(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "src/index.ts")
    }

    // ── Biome `invalid.js` fixtures: all three import forms of the exact
    //    `node:assert` specifier fire ─────────────────────────────────────────

    #[test]
    fn flags_static_import() {
        let diags = run_on("import assert from 'node:assert';");
        assert_eq!(diags.len(), 1, "unexpected: {diags:?}");
    }

    #[test]
    fn flags_dynamic_import() {
        let diags = run_on(r#"import("node:assert");"#);
        assert_eq!(diags.len(), 1, "unexpected: {diags:?}");
    }

    #[test]
    fn flags_require() {
        let diags = run_on(r#"require("node:assert");"#);
        assert_eq!(diags.len(), 1, "unexpected: {diags:?}");
    }

    #[test]
    fn flags_all_invalid_fixtures_together() {
        let src = "\
import assert from 'node:assert';
import(\"node:assert\");
require(\"node:assert\");";
        let diags = run_on(src);
        assert_eq!(diags.len(), 3, "unexpected: {diags:?}");
    }

    // ── Biome `valid.js` fixtures: a different specifier never fires ──────────

    #[test]
    fn allows_namespace_import_of_subpath() {
        // `node:assert/assert` is not the exact `node:assert` specifier.
        let diags = run_on(r#"import * as assert from "node:assert/assert";"#);
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn allows_dynamic_import_of_subpath() {
        let diags = run_on(r#"import("node:assert/assert");"#);
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn allows_require_of_subpath() {
        let diags = run_on(r#"require("node:assert/assert");"#);
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    // ── Specifier boundary cases ─────────────────────────────────────────────

    #[test]
    fn allows_already_strict_specifier() {
        // The whole point — `node:assert/strict` is the target, never flagged.
        let diags = run_on("import assert from 'node:assert/strict';");
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn allows_bare_assert_specifier() {
        // Biome matches the exact `node:` form only; bare `assert` is ambiguous
        // (could be a userland package) and is intentionally left alone.
        let diags = run_on("import assert from 'assert';");
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    // ── Other static import forms of the exact specifier ─────────────────────

    #[test]
    fn flags_named_import() {
        let diags = run_on("import { strict } from 'node:assert';");
        assert_eq!(diags.len(), 1, "unexpected: {diags:?}");
    }

    #[test]
    fn flags_namespace_import() {
        let diags = run_on("import * as assert from 'node:assert';");
        assert_eq!(diags.len(), 1, "unexpected: {diags:?}");
    }

    #[test]
    fn flags_side_effect_import() {
        let diags = run_on("import 'node:assert';");
        assert_eq!(diags.len(), 1, "unexpected: {diags:?}");
    }

    // ── Re-export forms (the `JsModuleSource` token Biome also queries) ───────

    #[test]
    fn flags_named_reexport() {
        let diags = run_on("export { strict } from 'node:assert';");
        assert_eq!(diags.len(), 1, "unexpected: {diags:?}");
    }

    #[test]
    fn flags_star_reexport() {
        let diags = run_on("export * from 'node:assert';");
        assert_eq!(diags.len(), 1, "unexpected: {diags:?}");
    }

    // ── Over-firing guards ───────────────────────────────────────────────────

    #[test]
    fn ignores_ambient_module_declaration_name() {
        // `declare module "node:assert"` declares an ambient module *named*
        // node:assert; the string is the module id, not an import of it. Biome
        // excludes it via `is_in_ts_module_declaration`; in oxc the string is
        // the `TSModuleDeclaration` id and never visited as an import source.
        let diags = run_on(r#"declare module "node:assert" { export function ok(v: unknown): void; }"#);
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn ignores_non_require_call_with_node_assert_arg() {
        // A call to something other than `require` that happens to take the
        // string is not an import.
        let diags = run_on(r#"doThing("node:assert");"#);
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }
}
