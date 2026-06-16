//! use-json-import-attributes oxc backend.
//!
//! Mirrors Biome `useJsonImportAttributes`: a default import of a module whose
//! source specifier ends in `.json` must carry `with { type: "json" }`.
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

    fn run_on(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "src/index.ts")
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
}
