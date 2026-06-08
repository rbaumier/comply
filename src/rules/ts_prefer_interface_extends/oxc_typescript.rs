//! ts-prefer-interface-extends oxc backend — flag `type X = A & B`
//! where every intersection member is a named type reference.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::TSType;
use std::sync::Arc;

pub struct Check;

fn is_named_type_ref(ty: &TSType) -> bool {
    matches!(ty, TSType::TSTypeReference(_))
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSTypeAliasDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSTypeAliasDeclaration(alias) = node.kind() else { return };

        let TSType::TSIntersectionType(intersection) = &alias.type_annotation else { return };

        if intersection.types.len() < 2 {
            return;
        }
        // Reject intersections containing shapes that `interface … extends` can't handle:
        // `Record<K, V>`, mapped types, or type literals with an index signature.
        if intersection.types.iter().any(|ty| match ty {
            TSType::TSTypeReference(ref_ty) => matches!(
                &ref_ty.type_name,
                oxc_ast::ast::TSTypeName::IdentifierReference(ident) if ident.name == "Record"
            ),
            TSType::TSMappedType(_) => true,
            TSType::TSTypeLiteral(lit) => lit
                .members
                .iter()
                .any(|m| matches!(m, oxc_ast::ast::TSSignature::TSIndexSignature(_))),
            _ => false,
        }) {
            return;
        }
        if !intersection.types.iter().all(is_named_type_ref) {
            return;
        }

        let name = alias.id.name.as_str();
        let (line, column) =
            byte_offset_to_line_col(ctx.source, alias.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Prefer `interface {name} extends ...` over `type {name} = A & B` for object composition."
            ),
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_intersection_of_named_types() {
        let diags = run_on("type X = A & B;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_intersection_of_generic_types() {
        let diags = run_on("type X = Base<T> & Mixin;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_intersection_with_object_literal() {
        assert!(run_on("type X = A & { extra: string };").is_empty());
    }

    #[test]
    fn allows_plain_type_alias() {
        assert!(run_on("type X = string;").is_empty());
    }

    /// Regression for #112 — `Record<K, V>` directly in the intersection
    /// would produce an `interface … extends Record<…>` rewrite that TS
    /// rejects (TS2310: index-signature aliases can't be extended).
    #[test]
    fn allows_intersection_with_record() {
        assert!(
            run_on("type AllowedFilters = Record<string, ZodType<unknown>> & ReservedFilterKeys;")
                .is_empty()
        );
    }

    #[test]
    fn allows_intersection_with_mapped_type() {
        assert!(
            run_on("type X = { [K in keyof T]: T[K] } & Y;").is_empty()
        );
    }

    #[test]
    fn allows_intersection_with_index_signature_literal() {
        assert!(
            run_on("type X = { [key: string]: number } & Y;").is_empty()
        );
    }
}
