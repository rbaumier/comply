//! ts-no-namespace oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Declaration, Statement, TSModuleDeclaration, TSModuleDeclarationBody};
use std::sync::Arc;

pub struct Check;

/// True when the namespace has a block body in which every statement is a
/// type-only declaration (a `type` alias or an `interface`, bare or exported).
///
/// Such a namespace introduces no runtime value — it is a type-grouping idiom
/// (e.g. `Schedule.Props`), not a legacy module-system construct, and cannot be
/// replaced by ES `export` / `import` without changing the consumer API.
/// Any value declaration, nested namespace, or other statement disqualifies it.
/// An empty body is type-only: TypeScript elides it at runtime.
fn is_type_only_namespace(decl: &TSModuleDeclaration<'_>) -> bool {
    let Some(TSModuleDeclarationBody::TSModuleBlock(block)) = &decl.body else {
        return false;
    };
    block.body.iter().all(|stmt| match stmt {
        Statement::TSTypeAliasDeclaration(_) | Statement::TSInterfaceDeclaration(_) => true,
        Statement::ExportNamedDeclaration(export) => matches!(
            export.declaration,
            Some(Declaration::TSTypeAliasDeclaration(_) | Declaration::TSInterfaceDeclaration(_))
        ),
        _ => false,
    })
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSModuleDeclaration]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["namespace"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSModuleDeclaration(decl) = node.kind() else { return };

        // Allow `declare namespace` (ambient declarations).
        if decl.declare {
            return;
        }

        // Allow `namespace NodeJS { ... }` — the prescribed mechanism for
        // augmenting Node.js built-in globals (e.g. `NodeJS.ProcessEnv`).
        // No ES module syntax can extend these ambient globals.
        if decl.id.name().as_str() == "NodeJS" {
            return;
        }

        // Allow type-only namespaces (a body of only `type` / `interface`
        // declarations). They introduce no runtime value and group types under
        // a co-named member API (e.g. `Schedule.Props`) that ES module syntax
        // cannot reproduce.
        if is_type_only_namespace(decl) {
            return;
        }

        // Only flag `namespace`, not `module`.
        let text = &ctx.source[decl.span.start as usize..decl.span.end as usize];
        if !text.starts_with("namespace") && !text.starts_with("export namespace") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, decl.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "TypeScript `namespace` is a legacy construct \u{2014} \
                      use ES module `export` / `import` instead."
                .into(),
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn allows_nodejs_global_augmentation() {
        let diags = run(
            "namespace NodeJS {\n  interface ProcessEnv {\n    AZURE_HTTP_USER_AGENT?: string;\n  }\n}",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_legacy_namespace() {
        assert_eq!(run("namespace Foo { export const x = 1; }").len(), 1);
    }

    #[test]
    fn allows_type_only_exported_namespace() {
        let diags = run(
            "export namespace Schedule {\n  export type Props = ScheduleProps;\n  export type StylesNames = ScheduleStylesNames;\n  export type Factory = ScheduleFactory;\n}",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_bare_type_only_namespace() {
        let diags = run("namespace N {\n  type A = B;\n  interface C { x: number }\n}");
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_mixed_type_and_value_namespace() {
        assert_eq!(
            run("namespace Mixed { export type A = B; export const c = 1; }").len(),
            1
        );
    }

    #[test]
    fn flags_nested_namespace() {
        assert_eq!(run("namespace Outer { export namespace Inner {} }").len(), 1);
    }
}
