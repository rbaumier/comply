//! use-json-import-attributes oxc backend.
//!
//! A default import of a module whose source specifier ends in `.json` must
//! carry `with { type: "json" }` — but only where the runtime actually requires
//! it. The attribute is enforced by TypeScript solely under Node's ESM module
//! system (`module`/`moduleResolution` `node16`/`node18`/`nodenext`) with an ESM
//! package scope (`"type":"module"`), so the rule fires only there
//! ([`crate::project::ProjectCtx::requires_node_esm_import_attributes`]). Under
//! bundler or `esnext` resolution, classic Node resolution, or CommonJS the JSON
//! import resolves without the attribute, so the rule stays silent.
//!
//! Two further exemptions keep it quiet where the attribute is neither required
//! nor conventional even under a Node-ESM tsconfig:
//!
//! - bundler-built projects (Vite/webpack/Rollup/…, detected by the shared
//!   [`crate::rules::file_extension_in_import::project_uses_bundler`] lever): the
//!   bundler resolves and inlines JSON at build time; and
//! - build-tool config files (`vite.config.ts`, `jest.config.ts`, … — anything
//!   matched by [`crate::rules::path_utils::is_config_file`]): resolved natively
//!   by the bundler/test runner (Vite, esbuild, Jest) at tool startup.
//!
//! When the attribute clause is present, the outcomes are:
//!
//! - No attribute clause at all                → "missing the `type: \"json\"`".
//! - A clause present but without a `type` key  → "missing `type: \"json\"`".
//! - A `type` key set to a non-`json` value     → left alone (the author has
//!   declared a deliberate, different module type).

use std::sync::Arc;

use oxc_ast::ast::{ImportAttributeKey, ImportDeclarationSpecifier};

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".json"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ImportDeclaration(import) = node.kind() else {
            return;
        };

        // Build-tool config files resolve JSON imports natively (Vite/esbuild/
        // Jest at tool startup), so the `type: "json"` attribute is neither
        // required nor conventional there.
        if crate::rules::path_utils::is_config_file(ctx.path) {
            return;
        }

        // Biome queries `JsImportDefaultClause`: only a default import is in
        // scope (`import x from './x.json'`). JSON modules expose a single
        // default export, so named/namespace/side-effect imports are not.
        let Some(specifiers) = &import.specifiers else {
            return;
        };
        let has_default = specifiers
            .iter()
            .any(|s| matches!(s, ImportDeclarationSpecifier::ImportDefaultSpecifier(_)));
        if !has_default {
            return;
        }

        if !import.source.value.as_str().ends_with(".json") {
            return;
        }

        // The `with { type: "json" }` attribute is required only under genuine
        // Node ESM (nearest tsconfig `module`/`moduleResolution`
        // node16/node18/nodenext with an ESM package scope). Under
        // bundler/`esnext` resolution, classic Node resolution, or CommonJS,
        // TypeScript resolves the JSON import without it.
        if !ctx.project.requires_node_esm_import_attributes(ctx.path) {
            return;
        }

        // A bundler-built project (Vite/webpack/Rollup/…) inlines JSON at build
        // time even under a Node-ESM tsconfig, so the attribute is still not
        // needed. This reuses the shared bundler lever directly rather than
        // through `ProjectCtx::cached_bundler`: that per-directory cache is
        // shared with `file_extension_in_import`, whose closure is broader
        // (bundler OR commonjs OR angular OR bundler-resolution); caching this
        // narrower predicate under the same directory key would poison the other
        // rule's lookups.
        if crate::rules::file_extension_in_import::project_uses_bundler(ctx) {
            return;
        }

        let message = match &import.with_clause {
            None => {
                "This JSON import is missing the `type: \"json\"` import attribute."
            }
            Some(clause) => {
                for entry in &clause.with_entries {
                    let key = match &entry.key {
                        ImportAttributeKey::Identifier(ident) => ident.name.as_str(),
                        ImportAttributeKey::StringLiteral(lit) => lit.value.as_str(),
                    };
                    if key == "type" {
                        // A `type` key is present: valid only when it is "json".
                        // Any other value is a deliberate, different module
                        // type and is left untouched.
                        return;
                    }
                }
                "The import attributes for this JSON module are missing `type: \"json\"`."
            }
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: message.to_string(),
            severity: Severity::Warning,
            span: Some((import.span.start as usize, import.span.size() as usize)),
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
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use std::fs;
    use tempfile::TempDir;

    /// Build a project rooted at a tempdir from the given files (relative paths),
    /// then run the OXC backend against `target_rel`. Lets a test stage the
    /// `package.json` + `tsconfig.json` that decide whether the JSON import
    /// attribute is required.
    fn run_with_files(files: &[(&str, &str)], target_rel: &str, source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        for (rel, contents) in files {
            let p = dir.path().join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, contents).unwrap();
        }
        let file_path = dir.path().join(target_rel);
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::write(&file_path, source).unwrap();
        let source_file = SourceFile {
            path: file_path.clone(),
            language: Language::from_path(&file_path).unwrap(),
        };
        let refs = vec![&source_file];
        let project = ProjectCtx::load(&refs, &Config::default());
        let canon = fs::canonicalize(&file_path).unwrap();
        crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            source,
            &canon,
            &project,
            crate::rules::file_ctx::default_static_file_ctx(),
        )
    }

    const NODE_ESM_PKG: &str = r#"{"type":"module"}"#;
    const NODE_ESM_TSCONFIG: &str =
        r#"{"compilerOptions":{"module":"nodenext","moduleResolution":"nodenext"}}"#;

    /// Run against `src/app.ts` in a genuine Node-ESM, non-bundler project
    /// (`"type":"module"` + tsconfig `module`/`moduleResolution: nodenext`) —
    /// the one setup where the `with { type: "json" }` attribute is required, so
    /// the fire path is exercised.
    fn run_on(src: &str) -> Vec<Diagnostic> {
        run_with_files(
            &[("package.json", NODE_ESM_PKG), ("tsconfig.json", NODE_ESM_TSCONFIG)],
            "src/app.ts",
            src,
        )
    }

    /// Same Node-ESM project, but the source lives at `path` (relative to the
    /// project root) — used to exercise the build-tool config-file exemption.
    fn run_on_path(src: &str, path: &str) -> Vec<Diagnostic> {
        run_with_files(
            &[("package.json", NODE_ESM_PKG), ("tsconfig.json", NODE_ESM_TSCONFIG)],
            path,
            src,
        )
    }

    // ── Biome `invalid.js` fixtures: a default JSON import without
    //    `type: "json"` fires ─────────────────────────────────────────────

    #[test]
    fn flags_default_json_import_without_clause() {
        let diags = run_on("import foo from 'bar.json';");
        assert_eq!(diags.len(), 1, "unexpected: {diags:?}");
        assert!(diags[0].message.contains("missing the `type: \"json\"`"));
    }

    #[test]
    fn flags_default_json_import_with_line_comment() {
        let diags = run_on("import foo from 'bar.json' // with comment\n");
        assert_eq!(diags.len(), 1, "unexpected: {diags:?}");
    }

    #[test]
    fn flags_default_json_import_with_trailing_comment() {
        let diags = run_on("import foo from 'bar.json'; // with comment after colon");
        assert_eq!(diags.len(), 1, "unexpected: {diags:?}");
    }

    #[test]
    fn flags_default_json_import_with_inline_comment() {
        let diags = run_on("import foo from 'bar.json'/** with inline comment */;");
        assert_eq!(diags.len(), 1, "unexpected: {diags:?}");
    }

    #[test]
    fn flags_clause_missing_type_key() {
        // `with { some: 'attr' }` — a clause exists but has no `type` key.
        let diags = run_on("import foo from 'bar.json' with { some: 'attr' };");
        assert_eq!(diags.len(), 1, "unexpected: {diags:?}");
        assert!(diags[0].message.contains("are missing `type: \"json\"`"));
    }

    #[test]
    fn flags_all_invalid_fixtures_together() {
        let src = "\
import foo from 'bar.json';

import foo2 from 'bar.json' // with comment

import foo3 from 'bar.json'; // with comment after colon

import foo4 from 'bar.json'/** with inline comment */;

import foo5 from 'bar.json' with { some: 'attr' };";
        let diags = run_on(src);
        assert_eq!(diags.len(), 5, "unexpected: {diags:?}");
    }

    // ── Biome `valid.js` fixtures: a correct `type: "json"` attribute is
    //    clean ─────────────────────────────────────────────────────────────

    #[test]
    fn allows_type_json() {
        let diags = run_on("import foo from 'bar.json' with { type: 'json' };");
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn allows_type_json_among_other_attributes() {
        let diags = run_on("import bar from 'baz.json' with { other: 'value', type: 'json' };");
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn allows_multiline_type_json() {
        let diags = run_on("import hoge from 'hoge.json' with {\n    type: 'json'\n};");
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn allows_all_valid_fixtures_together() {
        let src = "\
import foo from 'bar.json' with { type: 'json' };

import bar from 'baz.json' with { other: 'value', type: 'json' }

import hoge from 'hoge.json' with {
    type: 'json'
}";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    // ── Over-firing guards ─────────────────────────────────────────────────

    #[test]
    fn allows_non_json_import() {
        // From Biome's doc valid example: not a JSON import.
        let diags = run_on("import code from './script.js';");
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn allows_type_set_to_non_json() {
        // A deliberate, different module type is left untouched (Biome returns
        // None once it sees a `type` key whose value isn't "json").
        let diags = run_on("import sheet from 'styles.json' with { type: 'css' };");
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn ignores_named_only_json_import() {
        // No default specifier — out of scope per Biome's `JsImportDefaultClause`.
        let diags = run_on("import { foo } from 'bar.json';");
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn ignores_namespace_json_import() {
        let diags = run_on("import * as foo from 'bar.json';");
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn ignores_side_effect_json_import() {
        let diags = run_on("import 'bar.json';");
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn flags_default_with_named_combo() {
        // `import foo, { bar } from '...json'` carries a default specifier and
        // is therefore in scope.
        let diags = run_on("import foo, { bar } from 'data.json';");
        assert_eq!(diags.len(), 1, "unexpected: {diags:?}");
    }

    // ── Build-tool config files are exempt (issue #6942) ───────────────────

    #[test]
    fn ignores_missing_attribute_in_vite_config() {
        // A Vite config resolves the JSON import natively (esbuild at startup),
        // so a missing `type: "json"` attribute is not flagged.
        let diags = run_on_path(
            "import packageJson from './package.json'",
            "packages/react-query/vite.config.ts",
        );
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn ignores_missing_attribute_in_jest_config() {
        let diags = run_on_path("import foo from './bar.json';", "jest.config.ts");
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn flags_missing_attribute_in_regular_source_file() {
        // The same import in an ordinary source file is still flagged.
        let diags = run_on_path("import foo from './bar.json';", "src/index.ts");
        assert_eq!(diags.len(), 1, "unexpected: {diags:?}");
    }

    // ── Module-system gating (issue #7767) ─────────────────────────────────

    // Repro: nova-admin, a Vite + Vue SPA on `module:ESNext`,
    // `moduleResolution:node`. The skip is decided at the module-system gate:
    // ESNext is not Node's ESM system, so the attribute is not required (the
    // bundler check is never reached — that branch is covered separately by
    // `skips_json_import_in_bundler_project_under_nodenext_issue_7767`).
    #[test]
    fn skips_json_import_in_vite_esnext_project_issue_7767() {
        let diags = run_with_files(
            &[
                (
                    "package.json",
                    r#"{"type":"module","devDependencies":{"vite":"^5.0.0"}}"#,
                ),
                (
                    "tsconfig.json",
                    r#"{"compilerOptions":{"module":"ESNext","moduleResolution":"node","resolveJsonModule":true}}"#,
                ),
            ],
            "src/modules/i18n.ts",
            "import enUS from '../../locales/en_US.json';\n",
        );
        assert!(
            diags.is_empty(),
            "Vite + ESNext project resolves JSON natively — no attribute required: {diags:?}"
        );
    }

    // ESNext resolution alone (no bundler dependency) still does not enforce the
    // attribute, so the rule stays silent.
    #[test]
    fn skips_json_import_under_esnext_non_bundler_issue_7767() {
        let diags = run_with_files(
            &[
                ("package.json", r#"{"type":"module"}"#),
                (
                    "tsconfig.json",
                    r#"{"compilerOptions":{"module":"ESNext","moduleResolution":"node"}}"#,
                ),
            ],
            "src/app.ts",
            "import cfg from './cfg.json';\n",
        );
        assert!(
            diags.is_empty(),
            "ESNext resolution does not require the attribute: {diags:?}"
        );
    }

    // Positive fire path: a genuine Node-ESM project on `module:Node16` with no
    // bundler still requires the attribute — the gate narrows, it does not
    // neuter the rule.
    #[test]
    fn flags_json_import_under_node16_non_bundler_issue_7767() {
        let diags = run_with_files(
            &[
                ("package.json", r#"{"type":"module"}"#),
                (
                    "tsconfig.json",
                    r#"{"compilerOptions":{"module":"Node16","moduleResolution":"Node16"}}"#,
                ),
            ],
            "src/app.ts",
            "import data from './data.json';\n",
        );
        assert_eq!(
            diags.len(),
            1,
            "genuine Node-ESM (node16, non-bundler) requires the attribute: {diags:?}"
        );
    }

    // `module:node18` (TypeScript 5.8's pinned Node-ESM counterpart to
    // `nodenext`) enforces the attribute exactly like `node16`/`nodenext`, so a
    // genuine Node-ESM node18 project still fires.
    #[test]
    fn flags_json_import_under_node18_non_bundler_issue_7767() {
        let diags = run_with_files(
            &[
                ("package.json", r#"{"type":"module"}"#),
                (
                    "tsconfig.json",
                    r#"{"compilerOptions":{"module":"node18"}}"#,
                ),
            ],
            "src/app.ts",
            "import data from './data.json';\n",
        );
        assert_eq!(
            diags.len(),
            1,
            "genuine Node-ESM (node18, non-bundler) requires the attribute: {diags:?}"
        );
    }

    // A bundler project (Vite dependency) that happens to sit on a `nodenext`
    // tsconfig still inlines JSON at build time, so the attribute is not needed.
    #[test]
    fn skips_json_import_in_bundler_project_under_nodenext_issue_7767() {
        let diags = run_with_files(
            &[
                (
                    "package.json",
                    r#"{"type":"module","devDependencies":{"vite":"^5.0.0"}}"#,
                ),
                (
                    "tsconfig.json",
                    r#"{"compilerOptions":{"module":"NodeNext","moduleResolution":"NodeNext"}}"#,
                ),
            ],
            "src/app.ts",
            "import data from './data.json';\n",
        );
        assert!(
            diags.is_empty(),
            "a bundler project inlines JSON even under a nodenext tsconfig: {diags:?}"
        );
    }

    // A `node16` tsconfig with a CommonJS package scope (no `"type":"module"`)
    // makes each file CommonJS: the JSON import compiles to `require()`, which
    // needs no attribute, so the rule stays silent.
    #[test]
    fn skips_json_import_in_node16_commonjs_scope_issue_7767() {
        let diags = run_with_files(
            &[
                ("package.json", r#"{"name":"pkg"}"#),
                (
                    "tsconfig.json",
                    r#"{"compilerOptions":{"module":"Node16","moduleResolution":"Node16"}}"#,
                ),
            ],
            "src/app.ts",
            "import data from './data.json';\n",
        );
        assert!(
            diags.is_empty(),
            "node16 with a CommonJS package scope compiles the JSON import to require(): {diags:?}"
        );
    }
}
