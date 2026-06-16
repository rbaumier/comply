//! no-empty-type-parameters oxc backend for TypeScript / TSX.
//!
//! Flags an empty type-parameter list `<>` on a type alias or interface
//! declaration. oxc parses `type A<> = {}` / `interface B<> {}` into a
//! `TSTypeParameterDeclaration` with an empty `params` list; that empty
//! `<>` is the smell. A non-empty list or an absent list is accepted.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::TSTypeParameterDeclaration;
use std::sync::Arc;

pub struct Check;

/// Push a diagnostic when `type_params` is present but declares no
/// parameters — the empty `<>` form.
fn check_type_params(
    type_params: Option<&TSTypeParameterDeclaration>,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(type_params) = type_params else {
        return;
    };
    if !type_params.params.is_empty() {
        return;
    }
    let (line, column) = byte_offset_to_line_col(ctx.source, type_params.span.start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: "Empty type-parameter list `<>` is confusing; remove it or add a type parameter.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSTypeAliasDeclaration, AstType::TSInterfaceDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let type_params = match node.kind() {
            AstKind::TSTypeAliasDeclaration(alias) => alias.type_parameters.as_deref(),
            AstKind::TSInterfaceDeclaration(iface) => iface.type_parameters.as_deref(),
            _ => return,
        };
        check_type_params(type_params, ctx, diagnostics);
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
    fn flags_empty_type_alias_type_params() {
        // Biome invalid fixture: `type A<> = {};`
        assert_eq!(run_on("type A<> = {};").len(), 1);
    }

    #[test]
    fn flags_empty_interface_type_params() {
        // Biome invalid fixture: `interface B<> {};`
        assert_eq!(run_on("interface B<> {};").len(), 1);
    }

    #[test]
    fn allows_non_empty_type_alias_type_params() {
        // Biome valid fixture: `type A<T> = {};`
        assert!(run_on("type A<T> = {};").is_empty());
    }

    #[test]
    fn allows_non_empty_interface_type_params() {
        // Biome valid fixture: `interface B<T> {};`
        assert!(run_on("interface B<T> {};").is_empty());
    }

    #[test]
    fn allows_absent_type_params() {
        // No type-parameter list at all is the common, correct form.
        assert!(run_on("type Foo = X; interface Bar {}").is_empty());
    }
}
