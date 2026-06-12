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
}
