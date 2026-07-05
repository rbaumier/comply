//! ts-prefer-interface-extends oxc backend — flag `type X = A & B`
//! where every intersection member is a named type reference.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{TSType, TSTypeName};
use std::sync::Arc;

pub struct Check;

fn is_named_type_ref(ty: &TSType) -> bool {
    matches!(ty, TSType::TSTypeReference(_))
}

/// TypeScript built-in generics that carry a numeric index signature
/// (`[n: number]: T`). Merging two such bases into one `interface … extends`
/// — even with different names — produces conflicting index signatures that TS
/// rejects, so the intersection must stay a type alias.
const ARRAY_LIKE_INDEX_TYPES: &[&str] = &["Array", "ReadonlyArray"];

/// The base identifier name of an intersection member when it is a simple
/// `TSTypeReference` (e.g. `Array` in `Array<T>`), else `None`.
fn base_type_name<'a>(ty: &'a TSType) -> Option<&'a str> {
    match ty {
        TSType::TSTypeReference(ref_ty) => match &ref_ty.type_name {
            TSTypeName::IdentifierReference(ident) => Some(ident.name.as_str()),
            _ => None,
        },
        _ => None,
    }
}

/// True when `ty` is — or, through a top-level union/intersection, contains —
/// a primitive keyword type (`string`/`number`/`boolean`/`bigint`/`symbol`).
/// An `interface` cannot `extends` a type that includes a primitive (TS2312),
/// so an alias resolving to such a type is not a legal `extends` base.
fn contains_primitive_keyword(ty: &TSType) -> bool {
    match ty {
        TSType::TSStringKeyword(_)
        | TSType::TSNumberKeyword(_)
        | TSType::TSBooleanKeyword(_)
        | TSType::TSBigIntKeyword(_)
        | TSType::TSSymbolKeyword(_) => true,
        TSType::TSUnionType(u) => u.types.iter().any(contains_primitive_keyword),
        TSType::TSIntersectionType(i) => i.types.iter().any(contains_primitive_keyword),
        _ => false,
    }
}

/// True when intersection member `ty` is a `TSTypeReference` whose name resolves
/// to a declaration that `interface … extends` cannot legally name, so the whole
/// intersection must not be rewritten as an interface:
/// - a same-module type alias whose aliased type contains a primitive keyword
///   (e.g. `type FiniteNumber = number & Brand<…>`), which `extends` rejects; or
/// - an imported binding, whose definition is not visible here and so cannot be
///   confirmed to be an object type; or
/// - a type parameter (e.g. `N` in `type Merge<M, N> = … & N`), a bare type
///   variable that `interface … extends` cannot name (TS2312).
///
/// A reference that does not resolve (undeclared), or that resolves to an
/// in-module `interface`/`class`/object-typed alias, does not block the rewrite.
fn member_blocks_extends(ty: &TSType, semantic: &oxc_semantic::Semantic) -> bool {
    let TSType::TSTypeReference(ref_ty) = ty else { return false };
    let TSTypeName::IdentifierReference(ident) = &ref_ty.type_name else { return false };
    let Some(ref_id) = ident.reference_id.get() else { return false };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else { return false };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in
        std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        match kind {
            AstKind::TSTypeAliasDeclaration(alias) => {
                return contains_primitive_keyword(&alias.type_annotation);
            }
            AstKind::TSInterfaceDeclaration(_) | AstKind::Class(_) => return false,
            AstKind::ImportDeclaration(_) => return true,
            AstKind::TSTypeParameter(_) => return true,
            _ => {}
        }
    }
    false
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
        // Extending the same generic base twice (e.g. `Array<X> & Array<Y>`), or
        // two array-like bases whose numeric index signatures conflict (e.g.
        // `ReadonlyArray<X> & Array<Y>`), can't be expressed as one
        // `interface … extends`; TS rejects the merged index signatures.
        let base_names: Vec<&str> =
            intersection.types.iter().filter_map(base_type_name).collect();
        let has_duplicate_base = base_names
            .iter()
            .enumerate()
            .any(|(i, name)| base_names[i + 1..].contains(name));
        let multiple_array_like = base_names
            .iter()
            .copied()
            .filter(|name| ARRAY_LIKE_INDEX_TYPES.contains(name))
            .count()
            >= 2;
        if has_duplicate_base || multiple_array_like {
            return;
        }
        // A rewrite to `interface … extends` is only legal when every member is
        // an object type `extends` can name. Suppress when a member resolves to
        // a primitive-containing alias or to an unresolvable (imported) binding.
        if intersection
            .types
            .iter()
            .any(|ty| member_blocks_extends(ty, semantic))
        {
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

    /// Negative control: an intersection of two object interfaces declared in
    /// the same module is genuinely convertible and must still be flagged.
    #[test]
    fn flags_intersection_of_local_interfaces() {
        let diags = run_on("interface A { a: number } interface B { b: string } type X = A & B;");
        assert_eq!(diags.len(), 1);
    }

    /// A member aliasing a bare primitive can't be an `extends` base (TS2312).
    #[test]
    fn allows_intersection_with_primitive_alias_member() {
        assert!(run_on("type Id = string; type X = Id & Y;").is_empty());
    }

    /// An imported member's definition isn't visible, so we can't confirm it's
    /// an object type — suppress rather than emit a possibly-invalid rewrite.
    #[test]
    fn allows_intersection_with_imported_member() {
        assert!(
            run_on("import { Foo } from './foo';\ntype X = Foo & Bar;").is_empty()
        );
    }

    /// Regression for #6367 — extending the same generic base twice produces a
    /// conflicting numeric index signature, so TS rejects the interface rewrite.
    #[test]
    fn allows_intersection_of_duplicate_array_base() {
        assert!(run_on("type Type = Array<{x: string}> & Array<{z: number}>;").is_empty());
    }

    /// Regression for #6367 — two array-like bases (different names) each carry
    /// `[n: number]`, which conflict when merged into one interface.
    #[test]
    fn allows_intersection_of_readonly_array_and_array() {
        assert!(
            run_on("type Type = ReadonlyArray<{x: string}> & Array<{z: number}>;").is_empty()
        );
    }

    /// Regression for #6367 — nested object type args don't change the verdict;
    /// the duplicate `Array` base still blocks the rewrite.
    #[test]
    fn allows_intersection_of_duplicate_array_base_nested() {
        assert!(
            run_on("type Type = Array<{x: string}> & Array<{z: number; d: {e: string; f: boolean}}>;")
                .is_empty()
        );
    }

    /// Regression for #6495 — a branded numeric alias resolves through a
    /// same-module alias whose body is `number & Brand<…>`; rewriting the
    /// derived intersection as `interface … extends` is rejected by TS2312.
    #[test]
    fn allows_intersection_of_primitive_branded_aliases() {
        let src = "type Brand<Key extends string> = Readonly<Record<Key, true>>;\n\
            export type FiniteNumber = number & Brand<'__finiteNumberBrand'>;\n\
            export type Integer = FiniteNumber & Brand<'__integerBrand'>;";
        assert!(run_on(src).is_empty());
    }

    /// Regression for #7320 — `N` is a bare type parameter; `interface Merge<M, N>
    /// extends Omit<M, keyof N>, N {}` is rejected by TS2312, so the alias stays.
    #[test]
    fn allows_intersection_with_type_parameter_member() {
        assert!(run_on("export type Merge<M, N> = Omit<M, keyof N> & N;").is_empty());
    }

    /// Regression for #7320 — the leading member `T` is a type parameter, so the
    /// interface-extends rewrite would not compile.
    #[test]
    fn allows_intersection_leading_type_parameter_member() {
        assert!(run_on("export type LowInfer<T> = T & NonNullable<unknown>;").is_empty());
    }
}
