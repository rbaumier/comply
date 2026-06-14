//! OxcCheck backend for angular-no-barrel-file-side-effects — barrel `index.ts` should only re-export.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use crate::rules::path_utils::is_angular_schematic_or_migration_entry;
use oxc_ast::ast::Statement;
use std::sync::Arc;

pub struct Check;

fn is_barrel_path(path: &std::path::Path) -> bool {
    matches!(
        path.file_name().and_then(|n| n.to_str()),
        Some("index.ts") | Some("public-api.ts") | Some("public_api.ts")
    )
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !is_barrel_path(ctx.path) || is_angular_schematic_or_migration_entry(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        let program = semantic.source_text();
        let _ = program;
        for stmt in &semantic.nodes().program().body {
            let is_ok = matches!(
                stmt,
                Statement::ExportAllDeclaration(_)
                    | Statement::ExportNamedDeclaration(_)
                    | Statement::ExportDefaultDeclaration(_)
                    | Statement::ImportDeclaration(_)
                    | Statement::EmptyStatement(_)
            );
            if is_ok {
                continue;
            }
            let span = match stmt {
                Statement::ExpressionStatement(s) => s.span,
                Statement::BlockStatement(s) => s.span,
                Statement::VariableDeclaration(s) => s.span,
                Statement::FunctionDeclaration(s) => s.span,
                Statement::ClassDeclaration(s) => s.span,
                Statement::IfStatement(s) => s.span,
                Statement::ForStatement(s) => s.span,
                Statement::WhileStatement(s) => s.span,
                Statement::ReturnStatement(s) => s.span,
                Statement::ThrowStatement(s) => s.span,
                Statement::TryStatement(s) => s.span,
                Statement::SwitchStatement(s) => s.span,
                _ => oxc_span::Span::new(0, 0),
            };
            let snippet: String = ctx.source[span.start as usize..span.end as usize]
                .chars()
                .take(60)
                .collect();
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Barrel file should only re-export — found side-effecting statement: `{snippet}`."
                ),
                severity: Severity::Warning,
                span: None,
            });
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_at(src: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, path)
    }

    #[test]
    fn flags_side_effect_in_barrel() {
        let src = "export * from './a';\nconsole.log('side');";
        assert_eq!(run_at(src, "src/index.ts").len(), 1);
    }

    #[test]
    fn allows_pure_reexports() {
        let src = "export * from './a';\nexport { B } from './b';";
        assert!(run_at(src, "src/index.ts").is_empty());
    }

    #[test]
    fn ignores_non_barrel_files() {
        let src = "console.log('side');";
        assert!(run_at(src, "src/thing.ts").is_empty());
    }

    #[test]
    fn exempts_angular_schematic_entry_point() {
        // Issue #1597: ngrx/platform schematics/ng-add/index.ts is a default-export
        // factory entry point loaded by the Angular CLI, not a re-export barrel.
        let src = r#"
            import { Rule, SchematicContext, Tree, chain, noop } from '@angular-devkit/schematics';
            import { NodePackageInstallTask } from '@angular-devkit/schematics/tasks';
            function addModuleToPackageJson() {
                return (host: Tree, context: SchematicContext) => {
                    context.addTask(new NodePackageInstallTask());
                    return host;
                };
            }
            export default function (options: any): Rule {
                return (host: Tree, context: SchematicContext) => {
                    return chain([options.skipPackageJson ? noop() : addModuleToPackageJson()])(host, context);
                };
            }
        "#;
        assert!(run_at(src, "modules/effects/schematics/ng-add/index.ts").is_empty());
    }

    #[test]
    fn exempts_angular_migration_entry_point() {
        // Issue #1597: ng update migration entry point under migrations/X_Y_Z/index.ts.
        let src = r#"
            import { Rule, Tree } from '@angular-devkit/schematics';
            export default function (): Rule {
                return (tree: Tree) => { tree.create('flag', ''); return tree; };
            }
        "#;
        assert!(run_at(src, "modules/router-store/migrations/14_0_0/index.ts").is_empty());
    }

    #[test]
    fn still_flags_genuine_barrel_with_side_effects() {
        // Negative space: a real barrel index.ts outside schematics/migrations
        // with a side-effecting statement still fires.
        let src = "export { A } from './a';\nwindow.dataLayer = window.dataLayer || [];";
        assert_eq!(run_at(src, "src/components/index.ts").len(), 1);
    }
}
