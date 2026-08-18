//! Shared resolver for the `unknown`-discipline rules: does a written type
//! produce an `unknown` value?
//!
//! A type produces `unknown` when it is the keyword itself, a parenthesis or a
//! union around one, a `Promise`/`PromiseLike` of one, or an alias whose own
//! type produces one. Alias names resolve through the scope tree, so a type
//! parameter shadowing an alias name stops the walk instead of following it,
//! and a generic alias stops it too — what `Box<T>` resolves to depends on the
//! argument.

use oxc_ast::AstKind;
use oxc_ast::ast::{TSType, TSTypeName, TSTypeReference};
use oxc_semantic::{Semantic, SymbolId};
use rustc_hash::FxHashSet;

/// Wrappers whose first type argument is the value the type produces.
const AWAITABLE: &[&str] = &["Promise", "PromiseLike"];

/// True when a value of type `ty` is an `unknown` — an awaitable stands for
/// what it settles to.
#[must_use]
pub fn produces_unknown<'a>(ty: &TSType<'a>, semantic: &'a Semantic<'a>) -> bool {
    let mut visited = FxHashSet::default();
    produces(ty, semantic, &mut visited)
}

fn produces<'a>(
    ty: &TSType<'a>,
    semantic: &'a Semantic<'a>,
    visited: &mut FxHashSet<SymbolId>,
) -> bool {
    match ty {
        TSType::TSUnknownKeyword(_) => true,
        TSType::TSParenthesizedType(paren) => produces(&paren.type_annotation, semantic, visited),
        TSType::TSUnionType(union) => union
            .types
            .iter()
            .any(|member| produces(member, semantic, visited)),
        TSType::TSTypeReference(reference) => reference_produces(reference, semantic, visited),
        _ => false,
    }
}

/// A written name: an awaitable of some type, or an alias standing for one.
///
/// `visited` stops `type A = B; type B = A` from looping. It holds symbols, so
/// two same-named aliases in sibling scopes stay distinct.
fn reference_produces<'a>(
    reference: &TSTypeReference<'a>,
    semantic: &'a Semantic<'a>,
    visited: &mut FxHashSet<SymbolId>,
) -> bool {
    let TSTypeName::IdentifierReference(name) = &reference.type_name else {
        return false;
    };

    if let Some(arguments) = &reference.type_arguments {
        return AWAITABLE.contains(&name.name.as_str())
            && arguments
                .params
                .first()
                .is_some_and(|produced| produces(produced, semantic, visited));
    }

    let scoping = semantic.scoping();
    let Some(symbol) = name
        .reference_id
        .get()
        .and_then(|id| scoping.get_reference(id).symbol_id())
    else {
        return false;
    };
    if !visited.insert(symbol) {
        return false;
    }
    let AstKind::TSTypeAliasDeclaration(alias) =
        semantic.nodes().kind(scoping.symbol_declaration(symbol))
    else {
        return false;
    };
    alias.type_parameters.is_none() && produces(&alias.type_annotation, semantic, visited)
}
