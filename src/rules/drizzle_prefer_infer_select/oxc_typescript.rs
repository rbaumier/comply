//! drizzle-prefer-infer-select oxc backend — flag `InferSelectModel<...>` / `InferInsertModel<...>`.
//!
//! OXC does not expose `TSTypeReference` as a top-level `AstKind`, so we scan
//! type alias declarations and walk their type annotation trees.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::TSType;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["InferSelectModel", "InferInsertModel"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            if let AstKind::TSTypeAliasDeclaration(alias) = node.kind() {
                walk_type(&alias.type_annotation, ctx, &mut diagnostics);
            }
        }
        diagnostics
    }
}

fn walk_type(ty: &TSType, ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>) {
    match ty {
        TSType::TSTypeReference(ref_ty) => {
            let name = match &ref_ty.type_name {
                oxc_ast::ast::TSTypeName::IdentifierReference(ident) => ident.name.as_str(),
                _ => "",
            };
            if name == "InferSelectModel" || name == "InferInsertModel" {
                // Only flag if it has type parameters (i.e. it's a generic usage)
                if ref_ty.type_arguments.is_some() {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, ref_ty.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Use `typeof table.${}` instead of `{}<typeof table>`.",
                            if name == "InferSelectModel" {
                                "inferSelect"
                            } else {
                                "inferInsert"
                            },
                            name
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            // Recurse into type parameters
            if let Some(params) = &ref_ty.type_arguments {
                for param in &params.params {
                    walk_type(param, ctx, diagnostics);
                }
            }
        }
        TSType::TSUnionType(union) => {
            for member in &union.types {
                walk_type(member, ctx, diagnostics);
            }
        }
        TSType::TSIntersectionType(intersection) => {
            for member in &intersection.types {
                walk_type(member, ctx, diagnostics);
            }
        }
        TSType::TSArrayType(arr) => {
            walk_type(&arr.element_type, ctx, diagnostics);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_infer_select_model() {
        assert_eq!(run("type User = InferSelectModel<typeof users>").len(), 1);
    }


    #[test]
    fn flags_infer_insert_model() {
        assert_eq!(
            run("type NewUser = InferInsertModel<typeof users>").len(),
            1
        );
    }


    #[test]
    fn allows_infer_select_property() {
        assert!(run("type User = typeof users.$inferSelect").is_empty());
    }


    #[test]
    fn allows_unrelated_generic() {
        assert!(run("type X = Array<string>").is_empty());
    }
}
