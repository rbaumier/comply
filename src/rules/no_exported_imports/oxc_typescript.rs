//! no-exported-imports oxc backend — flag an imported binding that is
//! re-exported with a plain `export { … }` / `export default …` rather than a
//! direct `export … from "…"` re-export.
//!
//! A re-export specifier carrying a leading JSDoc block comment
//! (`export { /** … */ X }`) is exempt: `import * as X` + `export { X }` is the
//! only way in TypeScript to attach per-export JSDoc (`@category`/`@since`) to a
//! namespace re-export, since the suggested `export * as X from "…"` cannot
//! carry per-member documentation.
//!
//! A binding that is also referenced locally (in any position other than the
//! `export { X }` specifier itself — a value use, a type reference, etc.) is
//! exempt too: `export … from "…"` creates no local binding, so the suggested
//! rewrite would break every local use. Only a binding that is *solely*
//! re-exported can become a direct `export … from "…"`.

use std::sync::Arc;

use rustc_hash::FxHashSet;

use oxc_ast::ast::{
    ExportDefaultDeclarationKind, ImportDeclarationSpecifier, ModuleExportName,
};
use oxc_span::GetSpan;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ImportDeclaration(import) = node.kind() else {
            return;
        };
        let Some(specifiers) = &import.specifiers else {
            return;
        };

        let exported_locals = locally_exported_names(semantic);
        if exported_locals.is_empty() {
            return;
        }

        for specifier in specifiers {
            let (local_name, span, symbol_id) = match specifier {
                ImportDeclarationSpecifier::ImportSpecifier(named) => {
                    (named.local.name.as_str(), named.span, named.local.symbol_id.get())
                }
                ImportDeclarationSpecifier::ImportDefaultSpecifier(default) => {
                    (default.local.name.as_str(), default.local.span, default.local.symbol_id.get())
                }
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(ns) => {
                    (ns.local.name.as_str(), ns.span, ns.local.symbol_id.get())
                }
            };
            if exported_locals.contains(local_name) {
                // `export … from "…"` binds no local name, so the suggested
                // rewrite only applies when the import is *solely* re-exported.
                // A binding used anywhere else (value or type position) must keep
                // its `import`, so leave it alone.
                if binding_used_locally(symbol_id, semantic) {
                    continue;
                }
                let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`{local_name}` is imported then re-exported. Use `export … from \"…\"` to re-export it directly."
                    ),
                    severity: Severity::Warning,
                    span: Some((span.start as usize, span.size() as usize)),
                });
            }
        }
    }
}

/// True when the import binding `symbol_id` is referenced locally — in any
/// position other than the re-export occurrence itself.
///
/// Each resolved reference's direct parent is inspected. The two re-export
/// positions this rule flags have a distinctive parent: `export { X }` parents
/// the reference under an `ExportSpecifier`, and `export default X` under an
/// `ExportDefaultDeclaration`. Every other parent is a genuine local use — a
/// `StaticMemberExpression` (`X.foo()`), a `TSQualifiedName` (`X.Type`), a call
/// argument, etc. Type-position references count: `get_resolved_references`
/// yields them, and dropping the `import` would break a `X.Type` annotation just
/// as it would break a value use.
///
/// When `symbol_id` is `None` (no resolved binding), returns `false` so the
/// caller falls back to flagging.
fn binding_used_locally(
    symbol_id: Option<oxc_semantic::SymbolId>,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let Some(symbol_id) = symbol_id else {
        return false;
    };
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    scoping.get_resolved_references(symbol_id).any(|reference| {
        let parent_kind = nodes.kind(nodes.parent_id(reference.node_id()));
        !matches!(
            parent_kind,
            AstKind::ExportSpecifier(_) | AstKind::ExportDefaultDeclaration(_)
        )
    })
}

/// Collect the local binding names the module re-exports through a *plain*
/// export (`export { Name }`, `export default Name`) rather than a direct
/// re-export (`export { Name } from "mod"`). A direct re-export never binds a
/// local name — its specifiers reference the source module, not a local
/// binding — so its specifiers are excluded by skipping any
/// `ExportNamedDeclaration` that carries a `source`.
///
/// A specifier carrying a leading JSDoc block comment is also excluded: such a
/// re-export documents the binding per-member, which `export * as X from "…"`
/// cannot, so the import-then-export pattern is intentional there.
fn locally_exported_names<'a>(semantic: &'a oxc_semantic::Semantic<'a>) -> FxHashSet<&'a str> {
    let comments = semantic.comments();
    let source = semantic.source_text();
    let mut names = FxHashSet::default();
    for node in semantic.nodes().iter() {
        match node.kind() {
            AstKind::ExportNamedDeclaration(decl) => {
                // `export { A } from "mod"` is a direct re-export: its `A` names
                // an export of "mod", not a local binding, so it never re-exports
                // an import binding. Only plain `export { A }` (no source) does.
                if decl.source.is_some() {
                    continue;
                }
                for spec in &decl.specifiers {
                    if specifier_has_leading_jsdoc(
                        comments,
                        source,
                        spec.local.span().start as usize,
                    ) {
                        continue;
                    }
                    if let Some(name) = module_export_local_name(&spec.local) {
                        names.insert(name);
                    }
                }
            }
            AstKind::ExportDefaultDeclaration(decl) => {
                // `export default A` where `A` references an existing binding —
                // including an import binding. A default export of an inline
                // declaration (`export default function … {}`) or any non-bare
                // expression binds no name and is irrelevant here.
                if let ExportDefaultDeclarationKind::Identifier(reference) = &decl.declaration {
                    names.insert(reference.name.as_str());
                }
            }
            _ => {}
        }
    }
    names
}

fn module_export_local_name<'a>(name: &ModuleExportName<'a>) -> Option<&'a str> {
    match name {
        ModuleExportName::IdentifierReference(reference) => Some(reference.name.as_str()),
        ModuleExportName::IdentifierName(identifier) => Some(identifier.name.as_str()),
        ModuleExportName::StringLiteral(_) => None,
    }
}

/// True when the export specifier whose local name starts at `span_start` is
/// immediately preceded (whitespace-only gap) by a JSDoc block comment
/// (`/** … */`). Such a re-export carries per-member documentation that
/// `export * as X from "…"` cannot, so the import-then-export pattern is
/// intentional there. Matching against the real comment spans from
/// `semantic.comments()` keeps a `/**` that merely appears inside a string
/// literal from counting, and the whitespace-only gap check keeps a far-above
/// JSDoc that documents a different specifier from leaking onto this one. Only
/// `/**`-style block comments qualify — a plain `/* … */` or `//` does not.
fn specifier_has_leading_jsdoc(
    comments: &[oxc_ast::ast::Comment],
    source: &str,
    span_start: usize,
) -> bool {
    comments.iter().any(|comment| {
        let end = comment.span.end as usize;
        if end > span_start {
            return false;
        }
        if !source[end..span_start].chars().all(char::is_whitespace) {
            return false;
        }
        source[comment.span.start as usize..end].starts_with("/**")
    })
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

    // ── Biome `invalid.js` fixtures: imported-then-exported fires ──────────

    #[test]
    fn flags_named_import_then_export() {
        let diags = run_on("import { A } from \"mod\";\nexport { A };");
        assert_eq!(diags.len(), 1, "unexpected: {diags:?}");
    }

    #[test]
    fn flags_namespace_import_then_export() {
        let diags = run_on("import * as ns from \"mod\";\nexport { ns };");
        assert_eq!(diags.len(), 1, "unexpected: {diags:?}");
    }

    #[test]
    fn flags_default_import_then_export() {
        let diags = run_on("import D from \"mod\";\nexport { D };");
        assert_eq!(diags.len(), 1, "unexpected: {diags:?}");
    }

    #[test]
    fn flags_all_three_biome_invalid_fixtures_together() {
        // The full Biome `invalid.js` fixture: three diagnostics, one per import.
        let src = "\
import { A } from \"mod\";
export { A };

import * as ns from \"mod\";
export { ns };

import D from \"mod\";
export { D };";
        let diags = run_on(src);
        assert_eq!(diags.len(), 3, "unexpected: {diags:?}");
    }

    #[test]
    fn flags_renamed_export_of_import() {
        // `export { A as B }` still re-exports the import binding `A`.
        let diags = run_on("import { A } from \"mod\";\nexport { A as B };");
        assert_eq!(diags.len(), 1, "unexpected: {diags:?}");
    }

    #[test]
    fn flags_default_import_via_export_default() {
        // `export default D` re-exports the default-imported binding.
        let diags = run_on("import D from \"mod\";\nexport default D;");
        assert_eq!(diags.len(), 1, "unexpected: {diags:?}");
    }

    // ── Biome `valid.js` fixtures: direct re-exports are clean ─────────────

    #[test]
    fn allows_direct_named_re_export() {
        let diags = run_on("export { A } from \"mod\";");
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn allows_direct_namespace_re_export() {
        let diags = run_on("export * as ns from \"mod\";");
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn allows_direct_default_re_export() {
        let diags = run_on("export { default as D } from \"mod\";");
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn allows_all_three_biome_valid_fixtures_together() {
        let src = "\
export { A } from \"mod\";
export * as ns from \"mod\";
export { default as D } from \"mod\";";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    // ── Over-firing guards: locally-declared exports are clean ─────────────

    #[test]
    fn allows_export_of_local_declaration() {
        // `A` is declared locally, not imported — exporting it is fine.
        let diags = run_on("const A = 1;\nexport { A };");
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn allows_import_used_without_export() {
        // An import that is merely consumed locally is fine.
        let diags = run_on("import { A } from \"mod\";\nconsole.log(A);");
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn allows_export_from_alongside_local_import() {
        // A direct re-export from the same source must not flag the import of a
        // different binding.
        let diags = run_on("import { A } from \"mod\";\nexport { B } from \"mod\";\nconsole.log(A);");
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    // ── Locally-used (not solely re-exported) bindings are exempt (#4515) ───

    #[test]
    fn allows_namespace_import_used_in_type_then_re_exported() {
        // `z3` is referenced in a type position *and* re-exported. Rewriting to
        // `export * as z3 from "…"` would leave the `z3.ZodType` annotation
        // dangling, so the import-then-export pattern is required.
        let diags = run_on("import * as z3 from \"zod/v3\";\nexport type T = z3.ZodType;\nexport { z3 };");
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn allows_named_import_used_as_value_then_re_exported() {
        // `foo` is called locally and re-exported; converting to a direct
        // re-export would remove the local binding `foo` calls.
        let diags = run_on("import { foo } from \"m\";\nconst x = foo();\nexport { foo };");
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn allows_namespace_import_used_in_type_and_body_then_re_exported() {
        // The LlamaIndexTS `packages/core/src/zod/index.ts` repro shape: a
        // namespace import used in a type alias, a generic constraint, and a
        // function body, then re-exported. Every `z3`/`z4` use needs the local
        // binding, so the rule must stay silent.
        let src = "\
import * as z3 from \"zod/v3\";
import * as z4 from \"zod/v4/core\";
export type ZodSchema<T = any> = z3.ZodType<T> | z4.$ZodType<T>;
export function parseSchema<T>(schema: ZodSchema<T>, data: unknown): T {
  if (\"_zod\" in schema) {
    return z4.parse(schema as z4.$ZodType<T>, data);
  }
  return (schema as z3.ZodType<T>).parse(data);
}
export { z3, z4 };";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn still_flags_named_import_solely_re_exported() {
        // No local use: `bar` can become `export { bar } from "m"`. Proves the
        // exemption requires a genuine local usage, not mere co-occurrence.
        let diags = run_on("import { bar } from \"m\";\nexport { bar };");
        assert_eq!(diags.len(), 1, "unexpected: {diags:?}");
    }

    #[test]
    fn still_flags_namespace_import_solely_re_exported() {
        // `ns` is never referenced locally — `export * as ns from "m"` is the
        // correct rewrite, so the rule must still flag it.
        let diags = run_on("import * as ns from \"m\";\nexport { ns };");
        assert_eq!(diags.len(), 1, "unexpected: {diags:?}");
    }

    // ── JSDoc-annotated re-exports are exempt (fp-ts barrel pattern) ────────

    #[test]
    fn allows_jsdoc_annotated_namespace_re_export() {
        // `import * as X` + `export { /** … */ X }` is the only way to attach
        // per-export JSDoc to a namespace re-export; `export * as X from "…"`
        // cannot carry it, so the pattern is intentional.
        let src = "\
import * as alt from './Alt'
export {
  /**
   * @category model
   * @since 2.0.0
   */
  alt,
}";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn allows_jsdoc_annotated_re_exports_across_a_block() {
        // Each specifier in the block carries its own JSDoc — detection is
        // per-specifier, so both are exempt.
        let src = "\
import * as alt from './Alt'
import * as alternative from './Alternative'
export {
  /**
   * @category model
   * @since 2.0.0
   */
  alt,
  /**
   * @category model
   * @since 2.0.0
   */
  alternative,
}";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn exempts_only_the_jsdoc_annotated_specifier_in_a_block() {
        // `a` is JSDoc-annotated (exempt); `b` re-exports an import with no
        // leading JSDoc, so `b` is still flagged. Proves the exemption is
        // per-specifier, not whole-block.
        let src = "\
import * as a from './A'
import * as b from './B'
export {
  /** @category model */
  a,
  b,
}";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1, "unexpected: {diags:?}");
        assert!(
            diags[0].message.contains('b'),
            "the flagged binding should be `b`: {diags:?}"
        );
    }
}
