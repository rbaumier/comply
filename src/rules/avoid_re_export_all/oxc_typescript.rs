use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::path::Path;
use std::sync::Arc;

/// True when `path`'s file stem is a conventional public-API barrel name.
///
/// `index` is the universal barrel convention; `public-api` and `public_api`
/// are the ng-packagr Angular library convention (the source entry whose whole
/// job is to enumerate a package's exported surface via `export *`). Files with
/// these stems are deliberate barrels, so wildcard re-exports in them are the
/// intended contract, not a hidden surface.
fn is_public_api_barrel_stem(path: &Path) -> bool {
    matches!(
        path.file_stem().and_then(|s| s.to_str()),
        Some("index" | "public-api" | "public_api")
    )
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ExportAllDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ExportAllDeclaration(decl) = node.kind() else { return };
        // Allow namespace re-exports: `export * as ns from '...'`
        if decl.exported.is_some() {
            return;
        }
        // Barrel files use `export *` as their intentional public-API surface —
        // the entry point consumers import. `index.*` is the universal barrel
        // convention; `public-api.*` / `public_api.*` is the ng-packagr Angular
        // library convention for the same role. Only flag wildcard re-exports in
        // non-barrel modules, where they hide the module's surface.
        if is_public_api_barrel_stem(ctx.path) {
            return;
        }
        // A non-`index` file whose stem matches a published `package.json`
        // `exports` subpath (e.g. `src/static-functions.ts` for
        // `"./static-functions"`) is a declared public-API entry point that
        // aggregates a feature's surface via `export *` — the package's intended
        // contract, the same role the exempt `index.*` barrels serve.
        if ctx.project.is_declared_entry_barrel(ctx.path) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, decl.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Avoid `export * from '...'` \u{2014} use named re-exports instead.".into(),
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

    fn run_on(path: &str, src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, path)
    }

    #[test]
    fn flags_export_all_in_non_barrel_module() {
        let diags = run_on("src/helpers.ts", "export * from './internal.js'");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_namespace_re_export() {
        let diags = run_on("src/helpers.ts", "export * as ns from './internal.js'");
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn allows_export_all_in_barrel_index_issue_1058() {
        // Regression for issue #1058: `export *` in a barrel file (`index.ts`)
        // is the intentional public-API surface, not blind re-exporting.
        let src = "export * from './kysely.js'\nexport * from './query-creator.js'";
        let diags = run_on("src/index.ts", src);
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn allows_export_all_in_nested_barrel_index() {
        let diags = run_on("src/components/index.tsx", "export * from './Button.js'");
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn allows_export_all_in_angular_public_api_barrel_issue_1626() {
        // Regression for issue #1626: `public-api.ts` is the ng-packagr Angular
        // library convention for a package's public-API barrel — the same role
        // `index.*` serves — whose job is to enumerate the exported surface via
        // `export *`. Reproduces `src/cdk/tree/public-api.ts` from
        // angular/components.
        let src = "\
export * from './control/base-tree-control'
export * from './nested-node'
export * from './tree'
";
        let diags = run_on("src/cdk/tree/public-api.ts", src);
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn allows_export_all_in_public_api_underscore_barrel_issue_1626() {
        // ng-packagr also accepts the legacy `public_api.ts` spelling.
        let diags = run_on("src/lib/public_api.ts", "export * from './widget'");
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn flags_export_all_in_public_api_substring_file_issue_1626() {
        // Negative-space guard for issue #1626: only the exact `public-api` /
        // `public_api` stem is a barrel. An ordinary source file that merely
        // contains the substring in its name still hides its surface and fires.
        let diags = run_on("src/public-api-helpers.ts", "export * from './internal.js'");
        assert_eq!(diags.len(), 1, "expected ordinary file to be flagged: {diags:?}");
    }

    #[test]
    fn allows_export_all_in_non_index_file_declared_as_exports_entry_issue_1708() {
        // Regression for issue #1708: `src/static-functions.ts` aggregates a
        // feature's public surface via `export *` and is declared as the
        // `"./static-functions"` exports subpath, so it is an intentional
        // published entry point, not an accidental internal re-export hub.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"@tanstack/table-core","exports":{".":"./dist/index.js","./static-functions":"./dist/static-functions.js"}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        let path = dir.path().join("src/static-functions.ts");

        let src = "\
export * from './core/cells/coreCellsFeature.utils'
export * from './core/columns/coreColumnsFeature.utils'
export * from './core/headers/coreHeadersFeature.utils'
";
        let project = crate::project::ProjectCtx::empty();
        let file = crate::rules::file_ctx::default_static_file_ctx();
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, src, &path, &project, file);
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn flags_export_all_in_internal_hub_not_declared_in_exports_issue_1708() {
        // Negative-space guard for issue #1708: a non-`index` re-export hub that
        // is NOT a declared `exports` entry hides the module's surface and must
        // still be flagged, even though the package declares other exports.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"@tanstack/table-core","exports":{".":"./dist/index.js","./static-functions":"./dist/static-functions.js"}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        let path = dir.path().join("src/internal.ts");

        let src = "export * from './core/cells/coreCellsFeature.utils'\n";
        let project = crate::project::ProjectCtx::empty();
        let file = crate::rules::file_ctx::default_static_file_ctx();
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, src, &path, &project, file);
        assert_eq!(diags.len(), 1, "expected internal hub to be flagged: {diags:?}");
    }
}
