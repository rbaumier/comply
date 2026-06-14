//! OxcCheck backend for avoid-barrel-files.
//!
//! Uses `run_on_semantic` to scan the entire program for re-exports.
//! A file is a barrel when it has >= threshold re-export statements
//! and no other top-level code.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Statement;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // `index.*` barrels are the intentional public-API surface of a package
        // or namespace directory — the declared entry point consumers import.
        // Only flag pure re-export hubs in non-index modules, where a barrel is
        // an internal indirection rather than a published contract.
        if ctx.path.file_stem().and_then(|s| s.to_str()) == Some("index") {
            return Vec::new();
        }

        // A non-`index` file whose stem matches a published `package.json`
        // `exports` subpath (e.g. `src/production.ts` for `"./production"`) is a
        // declared public API entry point, not an accidental internal barrel —
        // re-exporting from it is the package's intended contract.
        if ctx.project.is_declared_entry_barrel(ctx.path) {
            return Vec::new();
        }

        let program = semantic.nodes().program();
        let barrel_threshold = ctx.config.threshold("avoid-barrel-files", "min_reexports", ctx.lang);

        let mut reexport_count = 0usize;

        for stmt in &program.body {
            match stmt {
                Statement::ExportNamedDeclaration(decl) => {
                    if decl.source.is_some() {
                        reexport_count += 1;
                    } else {
                        return Vec::new();
                    }
                }
                Statement::ExportAllDeclaration(_) => {
                    reexport_count += 1;
                }
                Statement::ExportDefaultDeclaration(_) => {
                    return Vec::new();
                }
                _ => {
                    return Vec::new();
                }
            }
        }

        if reexport_count < barrel_threshold {
            return Vec::new();
        }

        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: format!(
                "Barrel file — {reexport_count} re-exports and no other code. Import directly from source modules."
            ),
            severity: Severity::Warning,
            span: None,
        }]
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
    fn flags_pure_barrel_in_non_index_module() {
        let src = "\
export { a } from './a';
export { b } from './b';
export { c } from './c';
";
        assert_eq!(run_on("src/api.ts", src).len(), 1);
    }

    #[test]
    fn allows_package_entry_index_barrel_issue_1068() {
        // Regression for issue #1068: `common/tools/warp/src/index.ts` is the
        // package's published entry point — a re-export barrel by design.
        let src = "\
export { Logger, LogLevel } from './logger.js';
export { Pipeline } from './pipeline.js';
export { Runner } from './runner.js';
";
        let diags = run_on("common/tools/warp/src/index.ts", src);
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn allows_nested_namespace_index_barrel_issue_1068() {
        // Regression for issue #1068: a nested namespace barrel
        // (`operationsInterfaces/index.ts`) is the declared public surface of
        // that namespace, not an internal indirection hub.
        let src = "\
export { ManagementGroupSubscriptions } from './managementGroupSubscriptions.js';
export { ManagementGroups } from './managementGroups.js';
export { Entities } from './entities.js';
";
        let diags = run_on("sdk/managementgroups/arm-managementgroups/src/operationsInterfaces/index.ts", src);
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn allows_non_index_file_declared_as_exports_entry_issue_1707() {
        // Regression for issue #1707: `src/production.ts` re-exports the package's
        // public API and is declared as the `"./production"` exports subpath, so
        // it is an intentional published entry point, not an accidental barrel.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"preact-table-devtools","exports":{".":"./dist/index.js","./production":"./dist/production.js"}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        let path = dir.path().join("src/production.ts");

        let src = "\
export { TableDevtoolsPanel } from './PreactTableDevtools';
export { tableDevtoolsPlugin } from './plugin';
export { useTanStackTableDevtools } from './useTanStackTableDevtools';
";
        let project = crate::project::ProjectCtx::empty();
        let file = crate::rules::file_ctx::default_static_file_ctx();
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, src, &path, &project, file);
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn flags_internal_barrel_not_declared_in_exports_issue_1707() {
        // Negative-space guard for issue #1707: a sibling re-export hub that is
        // NOT a declared `exports` entry remains an internal indirection and must
        // still be flagged, even though the package declares other exports.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"preact-table-devtools","exports":{".":"./dist/index.js","./production":"./dist/production.js"}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        let path = dir.path().join("src/internal.ts");

        let src = "\
export { a } from './a';
export { b } from './b';
export { c } from './c';
";
        let project = crate::project::ProjectCtx::empty();
        let file = crate::rules::file_ctx::default_static_file_ctx();
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, src, &path, &project, file);
        assert_eq!(diags.len(), 1, "expected internal barrel to be flagged: {diags:?}");
    }
}
