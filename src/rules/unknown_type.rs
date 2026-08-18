//! Shared resolver for the `unknown`-discipline rules: is this written type
//! `unknown`?
//!
//! A type is `unknown` when it is the keyword itself, a parenthesis or a union
//! around one, or an alias whose own type is one. A caller asking what a value
//! of that type is — a return contract — reads a `Promise`/`PromiseLike`
//! through to its argument as well. Alias names resolve through the scope tree, so a type
//! parameter shadowing an alias name stops the walk instead of following it,
//! and a generic alias stops it too — what `Box<T>` resolves to depends on the
//! argument.

use oxc_ast::AstKind;
use oxc_ast::ast::{TSType, TSTypeName, TSTypeReference};
use oxc_semantic::{Semantic, SymbolId};
use rustc_hash::FxHashSet;

/// Wrappers whose first type argument is the value the type produces.
const AWAITABLE: &[&str] = &["Promise", "PromiseLike"];

/// What a walk is asking about.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// The written type. `Promise<unknown>` is a promise, not an `unknown`.
    Written,
    /// The value the type produces. An awaitable stands for what it settles to.
    Produced,
}

/// True when `ty` itself is `unknown`.
#[must_use]
pub fn resolves_to_unknown<'a>(ty: &TSType<'a>, semantic: &'a Semantic<'a>) -> bool {
    let mut visited = FxHashSet::default();
    walk(ty, semantic, Mode::Written, &mut visited)
}

/// True when a value of type `ty` is an `unknown`.
#[must_use]
pub fn produces_unknown<'a>(ty: &TSType<'a>, semantic: &'a Semantic<'a>) -> bool {
    let mut visited = FxHashSet::default();
    walk(ty, semantic, Mode::Produced, &mut visited)
}

fn walk<'a>(
    ty: &TSType<'a>,
    semantic: &'a Semantic<'a>,
    mode: Mode,
    visited: &mut FxHashSet<SymbolId>,
) -> bool {
    match ty {
        TSType::TSUnknownKeyword(_) => true,
        TSType::TSParenthesizedType(paren) => walk(&paren.type_annotation, semantic, mode, visited),
        TSType::TSUnionType(union) => union
            .types
            .iter()
            .any(|member| walk(member, semantic, mode, visited)),
        TSType::TSTypeReference(reference) => walk_reference(reference, semantic, mode, visited),
        _ => false,
    }
}

/// A written name: an alias standing for some type, or — when the caller asks
/// what a value is — an awaitable of one.
///
/// `visited` stops `type A = B; type B = A` from looping. It holds symbols, so
/// two same-named aliases in sibling scopes stay distinct.
fn walk_reference<'a>(
    reference: &TSTypeReference<'a>,
    semantic: &'a Semantic<'a>,
    mode: Mode,
    visited: &mut FxHashSet<SymbolId>,
) -> bool {
    let TSTypeName::IdentifierReference(name) = &reference.type_name else {
        return false;
    };

    if let Some(arguments) = &reference.type_arguments {
        return mode == Mode::Produced
            && AWAITABLE.contains(&name.name.as_str())
            && arguments
                .params
                .first()
                .is_some_and(|produced| walk(produced, semantic, mode, visited));
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
    alias.type_parameters.is_none() && walk(&alias.type_annotation, semantic, mode, visited)
}
