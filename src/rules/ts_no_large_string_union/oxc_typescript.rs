//! OxcCheck backend for ts-no-large-string-union — flag unions with >N string-literal members.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{PropertyKey, TSLiteral, TSSignature, TSType, TSTypeName};
use std::sync::Arc;

pub struct Check;

/// Property names that, by universal TypeScript tagged-union convention, hold a
/// variant's discriminant tag.
const DISCRIMINANT_PROP_NAMES: &[&str] = &["kind", "type", "tag", "variant"];

/// Count only string-literal union members. Numeric/boolean literals (e.g. HTTP
/// status-code unions) are not what a branded string or enum would replace, so
/// they must not count toward the threshold.
fn count_string_literals(ty: &TSType) -> usize {
    match ty {
        TSType::TSUnionType(union) => union.types.iter().map(count_string_literals).sum(),
        TSType::TSLiteralType(lit) => matches!(lit.literal, TSLiteral::StringLiteral(_)) as usize,
        _ => 0,
    }
}

/// True when `alias_name` is the discriminant register of a tagged union: some
/// interface or type-literal in the same file annotates a conventionally-named
/// discriminant property (`kind`/`type`/`tag`/`variant`) with a direct reference
/// to it. Such a union is the closed, exhaustive set of `node.kind === '...'`
/// tags — irreducible and structurally required by consumers — so its size is
/// inherent to the modelled AST, not an unbounded string set to brand or enumify.
///
/// The exemption keys on the union's usage position (referenced from a
/// discriminant property), not on its name or its string-literal member values.
fn is_discriminant_register(semantic: &oxc_semantic::Semantic, alias_name: &str) -> bool {
    semantic.nodes().iter().any(|node| {
        let members: &[TSSignature] = match node.kind() {
            AstKind::TSInterfaceDeclaration(decl) => &decl.body.body,
            AstKind::TSTypeLiteral(lit) => &lit.members,
            _ => return false,
        };
        members.iter().any(|member| {
            let TSSignature::TSPropertySignature(prop) = member else { return false };
            let PropertyKey::StaticIdentifier(key) = &prop.key else { return false };
            if !DISCRIMINANT_PROP_NAMES.contains(&key.name.as_str()) {
                return false;
            }
            let Some(annotation) = &prop.type_annotation else { return false };
            let TSType::TSTypeReference(type_ref) = &annotation.type_annotation else {
                return false;
            };
            let TSTypeName::IdentifierReference(ident) = &type_ref.type_name else { return false };
            ident.name.as_str() == alias_name
        })
    })
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSTypeAliasDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSTypeAliasDeclaration(alias) = node.kind() else { return };

        let TSType::TSUnionType(union) = &alias.type_annotation else { return };

        let max = ctx.config.threshold(super::META.id, "max", ctx.lang);
        let count: usize = union.types.iter().map(count_string_literals).sum();

        // A discriminated-union kind register (referenced by a discriminant
        // property in the same file) is a closed, irreducible tag set — exempt.
        if count > max && !is_discriminant_register(semantic, alias.id.name.as_str()) {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, union.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "String-literal union has {count} members (>{max}); consider a branded string or enum."
                ),
                severity: Severity::Error,
                span: None,
            });
        }
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

    fn string_union(n: usize) -> String {
        let members: Vec<String> = (0..n).map(|i| format!("'s{i}'")).collect();
        format!("type T = {};", members.join(" | "))
    }

    fn numeric_union(n: usize) -> String {
        let members: Vec<String> = (0..n).map(|i| i.to_string()).collect();
        format!("type T = {};", members.join(" | "))
    }

    fn named_string_union(name: &str, n: usize) -> String {
        let members: Vec<String> = (0..n).map(|i| format!("'s{i}'")).collect();
        format!("type {name} = {};", members.join(" | "))
    }

    #[test]
    fn exempts_numeric_dominated_http_status_union() {
        // openapi-typescript ErrorStatus: ~45 numeric codes, 3 string members.
        let src = r#"
export type ErrorStatus =
  500 | 501 | 502 | 503 | 504 | 505 | 506 | 507 | 508 | 510 | 511 | '5XX' |
  400 | 401 | 402 | 403 | 404 | 405 | 406 | 407 | 408 | 409 | 410 | 411 | 412 |
  413 | 414 | 415 | 416 | 417 | 418 | 420 | 421 | 422 | 423 | 424 | 425 | 426 |
  427 | 428 | 429 | 430 | 431 | 444 | 450 | 451 | 497 | 498 | 499 | '4XX' | "default";
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_large_purely_numeric_union() {
        assert!(run_on(&numeric_union(60)).is_empty());
    }

    #[test]
    fn flags_large_string_union() {
        assert_eq!(run_on(&string_union(60)).len(), 1);
    }

    #[test]
    fn flags_mixed_union_with_enough_string_members() {
        // 51 string members + 10 numeric members: still over the 50 threshold.
        let strings: Vec<String> = (0..51).map(|i| format!("'s{i}'")).collect();
        let numbers: Vec<String> = (0..10).map(|i| i.to_string()).collect();
        let src = format!("type T = {} | {};", strings.join(" | "), numbers.join(" | "));
        assert_eq!(run_on(&src).len(), 1);
    }

    #[test]
    fn ignores_small_string_union() {
        assert!(run_on(&string_union(50)).is_empty());
    }

    #[test]
    fn exempts_discriminated_union_kind_register() {
        // kysely OperationNodeKind: a 60-member string union used as the `kind`
        // discriminant of a same-file interface is a closed tagged-union register.
        let src = format!(
            "{}\ninterface OperationNode {{ readonly kind: OperationNodeKind }}",
            named_string_union("OperationNodeKind", 60)
        );
        assert!(run_on(&src).is_empty());
    }

    #[test]
    fn exempts_type_tag_variant_discriminants() {
        for prop in ["type", "tag", "variant"] {
            let src = format!(
                "{}\ntype Event = {{ {prop}: EventKind }};",
                named_string_union("EventKind", 60)
            );
            assert!(run_on(&src).is_empty(), "discriminant `{prop}` should exempt");
        }
    }

    #[test]
    fn flags_large_union_referenced_only_by_non_discriminant_property() {
        // `label` is not a discriminant property, so the union still flags.
        let src = format!(
            "{}\ninterface Widget {{ label: Labels }}",
            named_string_union("Labels", 60)
        );
        assert_eq!(run_on(&src).len(), 1);
    }

    #[test]
    fn flags_large_union_used_only_as_function_parameter() {
        // Not referenced by any interface/type-literal discriminant property.
        let src = format!(
            "{}\nfunction handle(k: Names): string {{ return k; }}",
            named_string_union("Names", 60)
        );
        assert_eq!(run_on(&src).len(), 1);
    }
}
