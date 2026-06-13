//! ts-no-shadow OXC backend — variable shadowing detection via oxc_semantic.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{
    byte_offset_to_line_col, is_type_only_binding_context, is_type_only_import_binding,
};
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::AstKind;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let scoping = semantic.scoping();
        let nodes = semantic.nodes();
        let mut diagnostics = Vec::new();

        for symbol_id in scoping.symbol_ids() {
            let scope_id = scoping.symbol_scope_id(symbol_id);
            let Some(parent_scope) = scoping.scope_parent_id(scope_id) else {
                continue;
            };
            let name = scoping.symbol_name(symbol_id);
            let decl_node = scoping.symbol_declaration(symbol_id);
            // Enum members are scoped inside the enum object and are only
            // reachable as `Enum.Member`, so they never shadow a module binding.
            if matches!(nodes.kind(decl_node), AstKind::TSEnumMember(_)) {
                continue;
            }
            if std::iter::once(nodes.kind(decl_node))
                .chain(nodes.ancestor_kinds(decl_node))
                .any(is_type_only_binding_context)
            {
                continue;
            }
            let ident = oxc_str::Ident::from(name);
            if let Some(outer_symbol) = scoping.find_binding(parent_scope, ident) {
                // A type-only import (`import type ...` or `import { type X }`) is
                // erased at compile time and creates no runtime binding, so a value
                // binding of the same name shadows nothing observable.
                let outer_decl = scoping.symbol_declaration(outer_symbol);
                if is_type_only_import_binding(nodes, outer_decl) {
                    continue;
                }
                let span = scoping.symbol_span(symbol_id);
                let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!("`{name}` is already declared in an outer scope."),
                    severity: Severity::Warning,
                    span: None,
                });
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn allows_index_signature_parameter_with_shadow() {
        let d = run_on("interface I { [key: string]: number } const key = \"x\";");
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn allows_mapped_type_key_with_shadow() {
        let d = run_on("type M<T> = { [K in keyof T]: T[K] }; const K = 1;");
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn allows_infer_type_parameter_with_shadow() {
        let d = run_on("type Unpack<T> = T extends Promise<infer R> ? R : never; const R = 1;");
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn allows_enum_member_matching_interface_name() {
        let d = run_on(
            "export enum KnownIdentityType {\n  \
             SystemAssignedIdentity = \"systemAssignedIdentity\",\n  \
             UserAssignedIdentity = \"userAssignedIdentity\",\n}\n\
             export interface UserAssignedIdentity { clientId?: string; }",
        );
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn still_flags_shadowing_in_real_function() {
        // Real function params still flag as shadows.
        let d = run_on("const x = 1; function f(x: number) { return x; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_param_shadowing_type_only_default_import() {
        // `import type yargs` is erased at runtime; a value param named `yargs`
        // shadows nothing observable.
        let d = run_on(
            "import type yargs from 'yargs';\n\
             export function builder(yargs: yargs.Argv) { return yargs; }",
        );
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn allows_param_shadowing_type_only_namespace_import() {
        let d = run_on(
            "import type * as yargs from 'yargs';\n\
             export function builder(yargs: yargs.Argv) { return yargs; }",
        );
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn allows_param_shadowing_type_only_named_import() {
        let d = run_on(
            "import { type Argv } from 'yargs';\n\
             export function builder(Argv: number) { return Argv; }",
        );
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn allows_param_shadowing_inline_type_named_import() {
        let d = run_on(
            "import type { Argv } from 'yargs';\n\
             export function builder(Argv: number) { return Argv; }",
        );
        assert!(d.is_empty(), "expected no diagnostics, got: {d:?}");
    }

    #[test]
    fn still_flags_param_shadowing_value_import() {
        // A real value import is a runtime binding, so shadowing it still fires.
        let d = run_on(
            "import yargs from 'yargs';\n\
             export function builder(yargs: number) { return yargs; }",
        );
        assert_eq!(d.len(), 1, "expected one diagnostic, got: {d:?}");
    }

    #[test]
    fn still_flags_param_shadowing_named_value_import() {
        let d = run_on(
            "import { Argv } from 'yargs';\n\
             export function builder(Argv: number) { return Argv; }",
        );
        assert_eq!(d.len(), 1, "expected one diagnostic, got: {d:?}");
    }
}
