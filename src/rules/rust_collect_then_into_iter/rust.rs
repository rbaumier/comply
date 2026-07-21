//! rust-collect-then-into-iter backend.
//!
//! Walks `call_expression` nodes whose function is
//! `<expr>.into_iter` and whose receiver expression is itself a
//! `call_expression` ending in `.collect::<Vec<_>>()`. Flags the chain
//! because the `Vec` is allocated only for `into_iter` to consume it
//! again — a no-op round-trip.
//!
//! The `collect` turbofish must name `Vec`. A `collect` without a
//! turbofish, or one that names another collection (`HashSet`,
//! `BTreeSet`, `IndexMap`, …), is left alone: those either cannot be
//! proven to be a `Vec` or carry semantics (dedup, ordering, keying)
//! that the round-trip preserves.
//!
//! The chain is left alone when the round-tripped iterator — directly in
//! the chain, or through the variable it is `let`-bound to — receives a
//! method needing more than `Iterator`: `DoubleEndedIterator` (`.rev()`,
//! `.next_back()`, …), `ExactSizeIterator` (`.len()`), or
//! `vec::IntoIter`'s inherent slice access. `vec::IntoIter` offers those
//! whatever the `Vec` was built from, whereas whether an adapter chain
//! does depends on element types the AST does not carry
//! (`Take<Rev<Chars>>` is not `DoubleEndedIterator`, so `.rev()` on it
//! does not compile). Which side of that line a chain falls on is
//! therefore not decidable here, so the exemption is deliberately
//! conservative: a chain that already satisfies the bound is left alone
//! too. The search stops at a `collect`, past which the receiver is a
//! container whose `len` or `rev` says nothing about the round-trip;
//! when the chain is instead `let`-bound, it carries on over the uses of
//! that binding while it is live, so a call on a shadowing rebinding of
//! the name does not count.
//!
//! The chain is also left alone when the owned iterator *escapes* its
//! scope — it is a `return`/function-tail value, a struct field
//! initializer, the right-hand side of an assignment, a `match`-arm value
//! whose `match` escapes, or a `let` binding whose bound variable later
//! feeds a struct-field initializer — rather than being consumed locally.
//! There the `Vec`
//! materialization is load-bearing: it breaks a borrow so the source's
//! owner can move into a downstream closure, or yields an owning
//! `IntoIter` of the right type for the escaping slot. An escaping
//! chain that immediately re-collects (`…into_iter().collect()`) is
//! still a genuine round-trip and is flagged.
//!
//! The chain is also left alone inside a test context (a `#[test]`
//! function or a `#[cfg(test)]` module): there the concrete
//! `Vec::IntoIter` type is routinely load-bearing, exercising the code
//! under a specific iterator *kind* (range vs slice-iter vs vec
//! into-iter vs custom) to cover branches gated on iterator-type
//! properties, so the round-trip selects a type rather than wasting an
//! allocation.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["call_expression"];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(func) = node.child_by_field_name("function") else {
            return;
        };
        // We need `<receiver>.into_iter` as the function.
        let (receiver, method) = match func.kind() {
            "field_expression" => {
                let value = func.child_by_field_name("value");
                let field = func.child_by_field_name("field");
                let Some(field) = field else { return };
                let name = field.utf8_text(source_bytes).unwrap_or("");
                if name != "into_iter" {
                    return;
                }
                (value, name)
            }
            "generic_function" => {
                // `.into_iter::<...>()` is not the typical form, skip.
                return;
            }
            _ => return,
        };
        let Some(receiver) = receiver else { return };
        if !receiver_is_collect_call(receiver, source_bytes) {
            return;
        }
        // The `Vec` is load-bearing when the round-tripped iterator receives a
        // method only the materialized collection is guaranteed to offer —
        // directly in the chain, or through the variable the chain is bound to.
        match chain_outcome(node, source_bytes) {
            ChainOutcome::NeedsMoreThanIterator => return,
            ChainOutcome::StillTheIterator => {
                if binding_needs_more_than_iterator(node, source_bytes) {
                    return;
                }
            }
            ChainOutcome::Collected => {}
        }
        // In a test context the concrete `Vec::IntoIter` type is load-bearing
        // (see module docs): the round-trip selects a type, not an allocation.
        if crate::rules::rust_helpers::is_in_test_context(node, source_bytes) {
            return;
        }
        // The owned iterator is load-bearing when it escapes its scope
        // without being immediately re-collected.
        if !into_iter_recollected(node, source_bytes) && chain_escapes(node, source_bytes) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            std::sync::Arc::clone(&ctx.path_arc),
            &node,
            "rust-collect-then-into-iter",
            format!(
                "`.collect::<Vec<_>>().{method}()` round-trips through a \
                 `Vec` for nothing. Drop both calls — the preceding chain \
                 is already an iterator."
            ),
            Severity::Error,
        ));
    }
}

/// Method names whose receiver must satisfy a bound stronger than
/// `Iterator`: the `DoubleEndedIterator` methods, `rposition`
/// (`DoubleEndedIterator + ExactSizeIterator`), `ExactSizeIterator`'s
/// `len`, and `vec::IntoIter`'s inherent slice accessors. `vec::IntoIter`
/// offers all of them whatever the `Vec` was built from, whereas whether
/// an adapter chain does depends on element types the AST does not carry
/// (`Take<Rev<Chars>>` is not `DoubleEndedIterator`).
const NEEDS_MORE_THAN_ITERATOR: &[&str] = &[
    "rev",
    "next_back",
    "nth_back",
    "rfind",
    "rfold",
    "try_rfold",
    "rposition",
    "len",
    "as_slice",
    "as_mut_slice",
];

/// What the method chain hanging off the `into_iter()` call does with the owned
/// iterator.
enum ChainOutcome {
    /// A link is one of `NEEDS_MORE_THAN_ITERATOR`.
    NeedsMoreThanIterator,
    /// The chain ends with its value still that iterator.
    StillTheIterator,
    /// A `collect` link ends the iterator: the chain's value is a container
    /// from there on, and its `len` or `rev` says nothing about the round-trip.
    Collected,
}

/// Reads the method chain hanging off `node`: `.method()` links, turbofished or
/// not, whose receiver is the chain so far. A call nested in an argument or a
/// closure has another receiver and is not a link.
fn chain_outcome(node: tree_sitter::Node, source: &[u8]) -> ChainOutcome {
    let mut current = node;
    while let Some(parent) = current.parent() {
        match parent.kind() {
            "field_expression" if parent.child_by_field_name("value") == Some(current) => {
                let Some(field) = parent.child_by_field_name("field") else {
                    return ChainOutcome::StillTheIterator;
                };
                let name = field.utf8_text(source).unwrap_or("");
                if name == "collect" {
                    return ChainOutcome::Collected;
                }
                if NEEDS_MORE_THAN_ITERATOR.contains(&name) {
                    return ChainOutcome::NeedsMoreThanIterator;
                }
            }
            // The call applying the link, and the turbofish it may carry.
            "call_expression" | "generic_function"
                if parent.child_by_field_name("function") == Some(current) => {}
            _ => return ChainOutcome::StillTheIterator,
        }
        current = parent;
    }
    ChainOutcome::StillTheIterator
}

/// True when the chain is bound by a `let` whose variable later receives one of
/// `NEEDS_MORE_THAN_ITERATOR` — `let mut it = …into_iter(); it.next_back();`.
/// The binding carries the same requirement as a directly chained call.
///
/// Call only when `chain_outcome` is `StillTheIterator`: past a `collect` the
/// binding holds the container, not the iterator.
///
/// Only uses over which this binding is live count (see `binding_live_range`),
/// so a same-named receiver inside the declaration itself (`let it = it.rev()…`)
/// does not answer for this iterator.
fn binding_needs_more_than_iterator(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(let_decl) = chain_outermost(node)
        .parent()
        .filter(|parent| parent.kind() == "let_declaration")
    else {
        return false;
    };
    let Some((name, scope)) = let_binding_name_and_scope(let_decl, source) else {
        return false;
    };
    let live = binding_live_range(scope, let_decl, name, source);
    any_named_use(scope, name, source, |ident| {
        live.contains(&ident.start_byte()) && receives_more_than_iterator(ident, source)
    })
}

/// The byte range over which a use of `name` in `scope` refers to the variable
/// `let_decl` binds: from the end of the declaration to the end of the next
/// declaration in the same block rebinding the name, or to the end of the
/// block. The rebinding's own initializer is inside the range — it is evaluated
/// before the name is rebound, so it still reads this variable. A rebinding
/// inside a nested block is not tracked.
fn binding_live_range(
    scope: tree_sitter::Node,
    let_decl: tree_sitter::Node,
    name: &str,
    source: &[u8],
) -> std::ops::Range<usize> {
    let start = let_decl.end_byte();
    let mut cursor = scope.walk();
    let end = scope
        .named_children(&mut cursor)
        .filter(|child| child.kind() == "let_declaration" && child.start_byte() >= start)
        .find(|child| let_binding_name(*child, source) == Some(name))
        .map_or(scope.end_byte(), |rebinding| rebinding.end_byte());
    start..end
}

/// True when `ident` is the receiver of one of `NEEDS_MORE_THAN_ITERATOR`.
fn receives_more_than_iterator(ident: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(parent) = ident.parent() else {
        return false;
    };
    if parent.kind() != "field_expression" || parent.child_by_field_name("value") != Some(ident) {
        return false;
    }
    let Some(field) = parent.child_by_field_name("field") else {
        return false;
    };
    NEEDS_MORE_THAN_ITERATOR.contains(&field.utf8_text(source).unwrap_or(""))
}

/// True when the `into_iter()` result is immediately re-collected —
/// `…collect::<Vec<_>>().into_iter().collect(…)`. That is a genuine
/// round-trip even when it escapes (e.g. returned), so it must still
/// flag. The shape: `node`'s parent is a `field_expression` whose
/// `value` is `node` and whose `field` is `collect`.
fn into_iter_recollected(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if parent.kind() != "field_expression" {
        return false;
    }
    if parent.child_by_field_name("value") != Some(node) {
        return false;
    }
    parent
        .child_by_field_name("field")
        .is_some_and(|f| f.utf8_text(source) == Ok("collect"))
}

/// True when the owned iterator (the `into_iter()` result plus any
/// downstream lazy adapters) escapes its local scope: it is a
/// `return`/function-tail value, a struct field initializer, the
/// right-hand side of an assignment, a `match`-arm value whose `match`
/// escapes, or a `let` binding whose bound variable later feeds a
/// struct-field initializer, rather than being consumed locally (a plain
/// `let` binding, a `for`/`while`/loop subject, or a discarded `;`
/// statement).
fn chain_escapes(node: tree_sitter::Node, source: &[u8]) -> bool {
    let outermost = chain_outermost(node);

    let mut current = outermost;
    while let Some(parent) = current.parent() {
        match parent.kind() {
            // Escape positions.
            "return_expression" => return true,
            "field_initializer" | "shorthand_field_initializer" => return true,
            // The right-hand side of an assignment: the LHS place has a fixed
            // concrete type, so the `into_iter()` conversion is load-bearing.
            // A `collect().into_iter()` is not an assignee expression, so
            // reaching this node from below always means we are in the RHS.
            "assignment_expression" => return true,
            // A match-arm value: the whole `match` takes the arm's type, so
            // continue the escape check from the enclosing `match_expression`.
            "match_arm" => match enclosing_match_expression(parent) {
                Some(match_expr) => current = match_expr,
                None => return false,
            },
            // A `let` binding escapes only when the bound variable later feeds
            // a struct-field initializer, filling a field of concrete
            // `vec::IntoIter` type; otherwise the iterator is consumed locally.
            "let_declaration" => return let_binding_feeds_struct_field(parent, source),
            "block" => {
                // The implicit-return tail is the block's last named child
                // with no trailing `;` — i.e. the child is the expression
                // itself, not an `expression_statement`.
                let mut cursor = parent.walk();
                let last_named = parent.named_children(&mut cursor).last();
                return last_named == Some(current);
            }
            // Expression wrappers: keep climbing, the value still flows out.
            "arguments"
            | "call_expression"
            | "tuple_expression"
            | "reference_expression"
            | "type_cast_expression"
            | "try_expression"
            | "await_expression"
            | "parenthesized_expression"
            | "unary_expression" => {
                current = parent;
            }
            // Any other context (let, for/while/loop, expression_statement,
            // …) consumes the iterator locally — not an escape.
            _ => return false,
        }
    }
    false
}

/// Walks up from the `into_iter()` call to the outermost expression of
/// its method chain: while the parent is a `field_expression` whose
/// `value` is the current node, or a `call_expression` — or the
/// `generic_function` carrying a link's turbofish — whose `function` is the
/// current node (the `.method().method()` continuation), the last such node
/// is the chain's outermost expression.
fn chain_outermost(node: tree_sitter::Node) -> tree_sitter::Node {
    let mut current = node;
    while let Some(parent) = current.parent() {
        let continues = match parent.kind() {
            "field_expression" => parent.child_by_field_name("value") == Some(current),
            "call_expression" | "generic_function" => {
                parent.child_by_field_name("function") == Some(current)
            }
            _ => false,
        };
        if continues {
            current = parent;
        } else {
            break;
        }
    }
    current
}

/// Walks up from a `match_arm` to its enclosing `match_expression`.
fn enclosing_match_expression(match_arm: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut current = match_arm;
    while let Some(parent) = current.parent() {
        if parent.kind() == "match_expression" {
            return Some(parent);
        }
        current = parent;
    }
    None
}

/// True when the `let` binding whose value is the chain feeds the bound
/// variable into a struct-field initializer somewhere in the enclosing
/// block (`Self { keys }` shorthand or `field: keys`). There the owned
/// `vec::IntoIter` fills a field of concrete type, so the `collect` is
/// load-bearing. A binding to `_`, a non-identifier pattern, or a variable
/// consumed locally (a `for` subject, a call argument, …) is not an escape.
/// Uses are matched by name across the block, so a later rebinding of the name
/// fed into a struct field counts as this binding's escape.
fn let_binding_feeds_struct_field(let_decl: tree_sitter::Node, source: &[u8]) -> bool {
    let Some((name, scope)) = let_binding_name_and_scope(let_decl, source) else {
        return false;
    };
    any_named_use(scope, name, source, is_struct_field_value)
}

/// The single variable a `let` binds, paired with the block that scopes it. A
/// `_`, tuple, or struct-destructuring pattern binds no single variable to
/// track and yields `None`, as does a `let` outside any block.
fn let_binding_name_and_scope<'tree, 'src>(
    let_decl: tree_sitter::Node<'tree>,
    source: &'src [u8],
) -> Option<(&'src str, tree_sitter::Node<'tree>)> {
    Some((
        let_binding_name(let_decl, source)?,
        enclosing_block(let_decl)?,
    ))
}

/// The single variable a `let` binds, or `None` for a pattern that binds no
/// single variable to track.
fn let_binding_name<'src>(let_decl: tree_sitter::Node, source: &'src [u8]) -> Option<&'src str> {
    let pattern = let_decl.child_by_field_name("pattern")?;
    let binding = binding_identifier(pattern)?;
    binding.utf8_text(source).ok()
}

/// Resolves a `let` pattern to its single bound identifier, unwrapping a
/// `mut` binding (`let mut x`). A `_`, tuple, or struct-destructuring
/// pattern has no single tracked binding and yields `None`.
fn binding_identifier(pattern: tree_sitter::Node) -> Option<tree_sitter::Node> {
    match pattern.kind() {
        "identifier" => Some(pattern),
        "mut_pattern" => {
            let mut cursor = pattern.walk();
            pattern
                .named_children(&mut cursor)
                .find(|child| child.kind() == "identifier")
        }
        _ => None,
    }
}

/// Nearest enclosing `block` of `node`.
fn enclosing_block(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "block" {
            return Some(parent);
        }
        current = parent;
    }
    None
}

/// True when some `identifier` named `name` anywhere under `scope` satisfies
/// `is_match`.
fn any_named_use(
    scope: tree_sitter::Node,
    name: &str,
    source: &[u8],
    is_match: impl Fn(tree_sitter::Node) -> bool,
) -> bool {
    let mut stack = vec![scope];
    while let Some(node) = stack.pop() {
        let names_the_binding = node.kind() == "identifier" && node.utf8_text(source) == Ok(name);
        if names_the_binding && is_match(node) {
            return true;
        }
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
    }
    false
}

/// True when `ident` sits directly in a struct-field initializer slot.
fn is_struct_field_value(ident: tree_sitter::Node) -> bool {
    let Some(parent) = ident.parent() else {
        return false;
    };
    match parent.kind() {
        "shorthand_field_initializer" => true,
        "field_initializer" => parent.child_by_field_name("value") == Some(ident),
        _ => false,
    }
}

/// True when `node` is a `<expr>.collect::<Vec<_>>()` call.
///
/// Requires the `generic_function` (turbofish) form whose first type
/// argument names `Vec`. A bare `.collect()` is not matched: without an
/// explicit type the collected type is inferred and cannot be proven to
/// be a `Vec`. Any other named collection (`HashSet`, `BTreeMap`, …) is
/// not matched either, since dropping the round-trip would change
/// semantics.
fn receiver_is_collect_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(func) = node.child_by_field_name("function") else {
        return false;
    };
    // Only `.collect::<Vec<_>>()` — a `generic_function` carrying a turbofish.
    if func.kind() != "generic_function" {
        return false;
    }
    let Some(field_expr) = func.child_by_field_name("function") else {
        return false;
    };
    if field_expr.kind() != "field_expression" {
        return false;
    }
    let Some(field) = field_expr.child_by_field_name("field") else {
        return false;
    };
    if field.utf8_text(source).unwrap_or("") != "collect" {
        return false;
    }
    turbofish_names_vec(func, source)
}

/// True when the turbofish on `generic_function` names `Vec` as its
/// outermost type — `collect::<Vec<_>>()`, `collect::<Vec<String>>()`,
/// or a path-qualified `collect::<std::vec::Vec<_>>()` (final segment
/// `Vec`).
fn turbofish_names_vec(generic_function: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(args) = generic_function.child_by_field_name("type_arguments") else {
        return false;
    };
    let mut cursor = args.walk();
    let Some(first_type) = args.named_children(&mut cursor).next() else {
        return false;
    };
    // `Vec<_>` parses as `generic_type` whose `type` field is the name.
    let type_name = match first_type.kind() {
        "generic_type" => match first_type.child_by_field_name("type") {
            Some(name) => name,
            None => return false,
        },
        // A bare `Vec` without arguments would be a plain identifier.
        "type_identifier" | "scoped_type_identifier" => first_type,
        _ => return false,
    };
    let text = type_name.utf8_text(source).unwrap_or("");
    text == "Vec" || text.rsplit("::").next() == Some("Vec")
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_collect_vec_then_into_iter() {
        let source = "fn f() { let _: Vec<_> = it.collect::<Vec<_>>().into_iter().collect(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_collect_vec_with_concrete_arg() {
        let source = "fn f() { let _ = xs.iter().map(f).collect::<Vec<String>>().into_iter(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_plain_collect_then_into_iter() {
        // No turbofish: the collected type is inferred and cannot be proven
        // to be a `Vec`, so dropping the round-trip may change semantics.
        let source = "fn f() { let _: Vec<_> = it.collect().into_iter().collect(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_collect_hashset_then_into_iter() {
        // The HashSet deduplicates; the round-trip is meaningful (issue #3265).
        let source = "fn f() { let _ = xs.iter().map(f).collect::<HashSet<_>>().into_iter().collect::<Vec<_>>().join(\"|\"); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_collect_btreeset_then_into_iter() {
        let source = "fn f() { let _ = xs.into_iter().collect::<BTreeSet<_>>().into_iter(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_collect_alone() {
        let source = "fn f() { let _: Vec<_> = it.collect(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_into_iter_on_vec_var() {
        let source = "fn f(v: Vec<u8>) { for x in v.into_iter() {} }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_other_method_after_collect() {
        let source = "fn f() { let n = it.collect::<Vec<_>>().len(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_escape_via_return_tail_before_move_closure() {
        // The submodule shape (issue #3715): the owned iterator escapes as
        // the function-tail value, and a downstream `move` closure captures
        // the owner. The `Vec` breaks the borrow — load-bearing.
        let source =
            "fn f() { Ok(Some(it.map(g).collect::<Vec<_>>().into_iter().map(move |n| X { n }))) }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_escape_via_explicit_return() {
        let source = "fn f() -> std::vec::IntoIter<u8> { return xs.iter().map(g).collect::<Vec<_>>().into_iter(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_escape_via_struct_field_store() {
        // The ripgrep shape: the owned iterator is stored in a struct field
        // of owning `IntoIter` type.
        let source =
            "fn f() -> W { W { its: xs.iter().map(g).collect::<Vec<_>>().into_iter() } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_escape_via_bare_function_tail() {
        let source = "fn f() -> std::vec::IntoIter<u8> { xs.iter().map(g).collect::<Vec<_>>().into_iter() }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_recollect_even_in_return_position() {
        // The owned iterator is immediately re-collected — a genuine
        // round-trip — even though the result escapes as the function tail.
        let source =
            "fn f() -> Vec<u8> { xs.iter().map(g).collect::<Vec<_>>().into_iter().collect() }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_for_loop_subject() {
        // Consumed locally by the loop — not an escape.
        let source = "fn f() { for x in xs.iter().map(g).collect::<Vec<_>>().into_iter() {} }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_bare_expression_statement() {
        // Discarded `;` statement — consumed locally, not an escape.
        let source = "fn f() { it.collect::<Vec<_>>().into_iter(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_escape_via_assignment_rhs_enum_variant() {
        // Issue #6459 (BurntSushi/walkdir): the chain flows into the RHS of
        // `*self = ...`; the `DirList::Closed` variant holds a concrete
        // `vec::IntoIter`, so the `into_iter()` conversion is load-bearing.
        let source = "fn close(&mut self) { *self = DirList::Closed(self.collect::<Vec<_>>().into_iter()); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_escape_via_assignment_rhs_simple() {
        // The LHS binding has a fixed concrete type; the round-trip yields the
        // exact `vec::IntoIter` that place requires.
        let source = "fn f(mut x: std::vec::IntoIter<u8>) { x = src.collect::<Vec<_>>().into_iter(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_round_trip_in_cfg_test_module() {
        // Issue #6198 (rust-random/rand, seq/iterator.rs): a test cycles an
        // algorithm through several iterator kinds; the `Vec::IntoIter` type
        // is deliberate, not a wasted allocation.
        let source = "#[cfg(test)]\nmod tests {\n    fn outer<R>(r: &mut R) {\n        test_iter(r, (0..9).collect::<Vec<_>>().into_iter());\n    }\n}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_round_trip_in_test_fn() {
        let source = "#[test]\nfn t() { let _: Vec<_> = it.collect::<Vec<_>>().into_iter().collect(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_round_trip_in_production_code() {
        // Negative space: the same round-trip in non-test production code with
        // no load-bearing-type reason is still the rule's genuine perf target.
        let source = "fn f() { let _: Vec<_> = it.collect::<Vec<_>>().into_iter().collect(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_escape_via_match_arm_into_struct_field() {
        // Issue #6894 case 1 (rust-lang/cargo): the chain is a match-arm value
        // bound to `keys` and stored in a struct field of concrete
        // `vec::IntoIter` type; the `collect` breaks the `map.keys()` borrow.
        let source = "fn new(cv: CV) -> Self { let keys = match &cv { CV::Table(map, _) => map.keys().cloned().collect::<Vec<_>>().into_iter(), _ => unreachable!() }; Self { cv, keys } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_escape_via_let_binding_into_struct_field() {
        // Issue #6894 case 2 (rust-lang/cargo): the chain is bound to `keys` and
        // stored in a struct field; the `collect` erases the slice-iter lifetime
        // to yield the concrete `vec::IntoIter<String>` the field requires.
        let source = "fn with_struct(cv: CV, given_fields: &[&str]) -> Self { let keys = given_fields.into_iter().map(|s| s.to_string()).collect::<Vec<_>>().into_iter(); Self { cv, keys } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_escape_via_mut_let_binding_into_struct_field() {
        // The same load-bearing escape through a `let mut` binding (iterator
        // state is often bound `mut` to be advanced later).
        let source = "fn with_struct(cv: CV, given_fields: &[&str]) -> Self { let mut keys = given_fields.into_iter().map(|s| s.to_string()).collect::<Vec<_>>().into_iter(); Self { cv, keys } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_rev_directly_after_into_iter() {
        // Issue #6814 (zellij, ui/text_utils.rs): `Take<Rev<Chars>>` is not
        // `DoubleEndedIterator`, so `.rev()` only compiles on the `Vec`'s
        // `IntoIter` — dropping the pair would break the build.
        let source = "fn f(text: &str, truncate_at: usize) -> String { text.chars().rev().take(truncate_at).collect::<Vec<_>>().into_iter().rev().collect() }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_rev_further_down_the_chain() {
        // `Map<I>` is `DoubleEndedIterator` only when `I` is, so the `Vec`
        // is still what makes the `.rev()` compile.
        let source = "fn f() { for x in it.take(n).collect::<Vec<_>>().into_iter().map(g).rev() {} }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_every_method_needing_more_than_iterator() {
        // The method names are the gate's interface: each one only resolves on
        // the materialized `vec::IntoIter`, so all of them must exempt.
        let methods = [
            "rev",
            "next_back",
            "nth_back",
            "rfind",
            "rfold",
            "try_rfold",
            "rposition",
            "len",
            "as_slice",
            "as_mut_slice",
        ];
        assert_eq!(
            methods.len(),
            NEEDS_MORE_THAN_ITERATOR.len(),
            "a new gated method must be listed here too"
        );
        for method in methods {
            let source =
                format!("fn f() {{ let _ = it.take(n).collect::<Vec<_>>().into_iter().{method}(); }}");
            assert!(run_on(&source).is_empty(), "`.{method}()` must exempt");
        }
    }

    #[test]
    fn allows_capability_method_reached_through_a_turbofished_link() {
        // A turbofished adapter is still a link of the chain, so the `.rev()`
        // behind it is still the chain's requirement.
        let source =
            "fn f() { let _ = it.take(n).collect::<Vec<_>>().into_iter().map::<u32, _>(g).rev().count(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_binding_whose_later_use_needs_more_than_iterator() {
        // The requirement can sit one binding away: `next_back` only resolves
        // because `it` is the `Vec`'s `IntoIter`.
        let source = "fn f(text: &str, n: usize) { let mut it = text.chars().rev().take(n).collect::<Vec<_>>().into_iter(); while let Some(c) = it.next_back() { drop(c); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_binding_reached_through_a_turbofished_tail_link() {
        // The chain ends on a turbofished adapter, so finding the binding means
        // walking through the `generic_function` that carries the turbofish.
        let source = "fn f(text: &str, n: usize) { let mut it = text.chars().rev().take(n).collect::<Vec<_>>().into_iter().map::<char, _>(g); it.next_back(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_binding_read_by_its_own_rebinding() {
        // `let it = it.rev();` reads the round-tripped iterator before the name
        // is rebound, so the `.rev()` is still this chain's requirement.
        let source = "fn f(text: &str, n: usize) { let it = text.chars().rev().take(n).collect::<Vec<_>>().into_iter(); let it = it.rev(); for c in it { drop(c); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_escape_via_turbofished_tail_in_return_position() {
        // The escape check must reach the chain's outermost expression through
        // the turbofish too, or the tail is not seen as escaping.
        let source = "fn f() -> impl Iterator<Item = u32> { xs.iter().collect::<Vec<_>>().into_iter().map::<u32, _>(g) }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_binding_of_a_chain_that_ends_in_a_collect() {
        // Negative space: the `let` binds the collected `Vec`, so `words.len()`
        // is `Vec::len` — the round-trip inside the chain is still wasted.
        let source = "fn f(line: &str) { let words = line.split(' ').collect::<Vec<_>>().into_iter().map(norm).collect::<Vec<_>>(); consume(words.len()); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_when_the_binding_shadows_the_receiver_of_an_upstream_rev() {
        // Negative space: the `.rev()` belongs to the *previous* `it`, applied
        // before the round-trip — the binding under test is never a `rev`
        // receiver, so the wasted allocation must still be reported.
        let source = "fn f(it: Chars) { let it = it.rev().collect::<Vec<_>>().into_iter(); for c in it { drop(c); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_when_only_a_later_rebinding_needs_more_than_iterator() {
        // Negative space: `data.len()` reads the `Vec` bound after the chain,
        // not the round-tripped iterator.
        let source = "fn f() { let data = xs.iter().collect::<Vec<_>>().into_iter(); consume(data); let data = vec![1, 2]; consume(data.len()); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_capability_method_after_a_downstream_collect() {
        // Negative space: past the second `collect` the receiver is a `Vec`, so
        // `.len()` is `Vec::len` — the round-trip remains a wasted allocation.
        let source =
            "fn f() { let n = it.take(k).collect::<Vec<_>>().into_iter().collect::<Vec<_>>().len(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_when_rev_only_precedes_the_collect() {
        // Negative space: a `.rev()` upstream of the `collect` is already
        // applied — it says nothing about what the round-trip enables.
        let source = "fn f() { let _: Vec<_> = xs.iter().rev().collect::<Vec<_>>().into_iter().collect(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_when_rev_is_inside_a_downstream_closure() {
        // Negative space: `.rev()` on another receiver inside a closure is not
        // a link of the chain, so the round-trip stays a wasted allocation.
        let source = "fn f() { let _: Vec<_> = it.collect::<Vec<_>>().into_iter().map(|s| s.chars().rev()).collect(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_let_binding_consumed_locally() {
        // A let-bound chain whose variable is consumed locally (a `for` subject,
        // not a struct field) is still a genuine round-trip — the precise
        // let-escape check must not exempt it.
        let source = "fn f() { let v = xs.iter().map(g).collect::<Vec<_>>().into_iter(); for x in v {} }";
        assert_eq!(run_on(source).len(), 1);
    }
}
