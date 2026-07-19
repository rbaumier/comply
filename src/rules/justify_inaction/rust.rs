//! justify-inaction Rust backend.
//!
//! Flags empty control-flow blocks that have no comment inside
//! explaining why. Targets:
//!
//! - `if_expression.consequence` — empty `if cond { }`.
//! - `else_clause`'s `block` child — empty `else { }`.
//! - `match_arm.value` when the value is an empty `block` AND the
//!   pattern is an error-ignoring `Err(…)` — silently swallowing an
//!   error deserves a justification. Wildcard arms (`_ => {}`) and
//!   named variant no-ops (`Progress::None => {}`) are exempt: the
//!   explicit catch-all / variant name documents the intent, and the
//!   wildcard arm is required for match exhaustiveness anyway.
//!   An `Err(…)` arm is also exempt when every variable it binds is
//!   underscore-prefixed (e.g. `Err(_frame) => {}`): the `_` prefix is
//!   Rust's signal that the value is intentionally discarded, so the
//!   empty body is already documented by the binding name. It is likewise
//!   exempt when its payload is a const/path that binds nothing
//!   (`Err(Self::REGISTERED) => {}`, `Err(MAX_RETRIES) => {}`): the arm
//!   pins itself to one specific known value — the self-documenting
//!   lock-free CAS "already in this exact state" no-op — just like a
//!   named-variant arm. Finally, an arm is exempt when its guard
//!   (`match_pattern.condition`) references `WouldBlock`
//!   (`Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}`): this is
//!   the canonical non-blocking I/O reactor signal where the correct
//!   action is to do nothing and let the loop await readability, so the
//!   guard itself documents the inaction.
//!   An or-pattern enumerating multiple explicit, non-wildcard alternatives
//!   (`A | B | StmtKind::Err(_) => {}`) is also exempt: listing every case by
//!   name proves the author considered each one, so the empty body is correct
//!   by construction — this is stronger documentation than a wildcard. The
//!   exemption is withheld when the or-pattern includes a catch-all alternative
//!   (`A | _ => {}`, `A | other => {}`) that hides the remaining cases.
//!   An empty arm is also exempt when a sibling CATCH-ALL arm in the same
//!   `match` diverges — an unguarded `_`/wildcard/bare-binding arm whose body
//!   cannot fall through: a `panic!`/`unreachable!`/`todo!`/`unimplemented!`
//!   macro, or a `return`/`break`/`continue` (bare `_ => panic!(…)` or braced
//!   `_ => { panic!(…) }`). A diverging catch-all sibling makes the `match` an
//!   assertion/guard: it absorbs and aborts on every case the other arms do not
//!   match, so the empty arm is unambiguously the intended (expected) case, not
//!   a forgotten no-op. Requiring the diverging arm to be a catch-all is what
//!   keeps a plain error-swallow like `Ok(v) => return v, Err(e) => {}` flagged
//!   — there the diverging `Ok` arm is a specific variant and the empty `Err`
//!   arm still silently swallows. Keyed on the enclosing `match`'s arms, not on
//!   the empty arm's own pattern.
//! - `for_expression.body` / `while_expression.body` /
//!   `loop_expression.body` — empty loop body.
//!
//! A block is considered justified and NOT flagged if it contains at
//! least one comment child (`line_comment` / `block_comment`). Any
//! other named child also makes the block non-empty by definition.
//!
//! For loops (`for` / `while` / `loop`) the justification may also sit
//! on the line(s) immediately preceding the loop, with no blank line in
//! between — the idiomatic place for an iterator-drain comment where the
//! work happens in the condition and the body is intentionally empty
//! (e.g. `while it.next_if(p).is_some() {}`).
//!
//! An empty-bodied `while` is additionally exempt when its condition
//! contains a call expression (`while !peripheral.ready() {}`,
//! `while reg.read().bit() != X {}`, `while set.join_next().await.is_some() {}`).
//! The call makes the condition self-documenting: it is the standard
//! embedded register-polling / iterator-drain idiom where all the work
//! happens in the condition and the body is empty by design, so a body
//! comment would only restate the condition. A condition with no call —
//! a bare flag (`while running {}`), a literal (`while true {}`), or a
//! pure comparison (`while x < n {}`) — is not self-documenting and is
//! still flagged.
//!
//! An empty-bodied `for` is exempt when its pattern is a bare wildcard
//! (`for _ in iter.by_ref().take(3) {}`): draining an iterator for its
//! side effects is the idiomatic use of such a loop and the `_` documents
//! that the yielded value is intentionally ignored, exactly like a
//! wildcard `match` arm. A named binding (`for x in iter {}`) or a
//! destructuring pattern (`for (k, v) in map {}`) still flags.
//!
//! Function bodies, closure bodies, and empty `{}` used as unit
//! expressions in other positions are out of scope — they are common
//! in stubs, marker impls, and no-op callbacks, and flagging them
//! would be pure noise.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{block_diverges, node_diverges, tuple_struct_pattern_binds_const};

fn block_is_empty(node: tree_sitter::Node) -> bool {
    node.kind() == "block" && node.named_child_count() == 0
}

fn match_arm_needs_justification(arm: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(pattern) = arm.child_by_field_name("pattern") else {
        return true;
    };
    if guard_is_self_documenting(pattern, source) {
        return false;
    }
    if sibling_arm_diverges(arm, source) {
        return false;
    }
    pattern_needs_justification(pattern, source)
}

/// True when a sibling CATCH-ALL arm in the same `match` diverges — its body
/// cannot fall through: a `panic!`/`unreachable!`/`todo!`/`unimplemented!` macro,
/// or a `return`/`break`/`continue`, either as a bare arm value (`_ => panic!(…)`)
/// or a braced block whose tail diverges (`_ => { panic!(…) }`). A diverging
/// catch-all sibling turns the `match` into an assertion/guard: it absorbs and
/// aborts on every case the other arms do not match, so the empty arm is
/// unambiguously the intended (expected) case and needs no comment. The catch-all
/// requirement is what keeps a specific-variant divergence like
/// `Ok(v) => return v, Err(e) => {}` flagged — there the empty `Err` arm still
/// silently swallows. Keyed on the enclosing `match_block`'s arms — a structural
/// property of the `match` — not on the empty arm's own pattern. Reuses the shared
/// divergence predicates (`block_diverges` / `node_diverges`).
fn sibling_arm_diverges(arm: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(match_block) = arm.parent() else {
        return false;
    };
    let mut cursor = match_block.walk();
    match_block.children(&mut cursor).any(|sibling| {
        sibling.id() != arm.id()
            && sibling.kind() == "match_arm"
            && arm_is_catch_all(sibling, source)
            && sibling.child_by_field_name("value").is_some_and(|value| {
                block_diverges(value, source) || node_diverges(value, source)
            })
    })
}

/// True when a `match_arm` is an unconditional catch-all — an unguarded `_`,
/// `wildcard_pattern`, or bare fresh-binding pattern (`_ => …`, `res => …`) — so
/// it absorbs every case the other arms do not match. A bare `identifier` is a
/// catch-all only when it is a binding: `_`-prefixed or lowercase-leading; an
/// uppercase-leading identifier (`None => …`) is a unit-variant reference, not a
/// catch-all (same leading-case convention as `or_pattern_has_wildcard`). A
/// guarded arm (`res if cond => …`) can fail its guard, so it is not an
/// unconditional catch-all.
fn arm_is_catch_all(arm: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(pattern) = arm.child_by_field_name("pattern") else {
        return false;
    };
    // The guard, when present, lives on the `match_pattern` wrapper; a guarded
    // arm can fail its guard, so it is not an unconditional catch-all.
    if pattern.child_by_field_name("condition").is_some() {
        return false;
    }
    // The arm pattern is wrapped in a `match_pattern`; the real pattern is its
    // first child. Use `child`, not `named_child`: the wildcard `_` is an
    // anonymous token and would be skipped by `named_child`.
    let inner = match pattern.kind() {
        "match_pattern" => match pattern.child(0) {
            Some(first) => first,
            None => return false,
        },
        _ => pattern,
    };
    match inner.kind() {
        "_" | "wildcard_pattern" => true,
        "identifier" => inner
            .utf8_text(source)
            .is_ok_and(|name| name.starts_with('_') || name.starts_with(char::is_lowercase)),
        _ => false,
    }
}

/// True when the arm's guard (`match_pattern.condition`) references `WouldBlock`,
/// the canonical non-blocking I/O signal. The guard makes the empty body the
/// self-documenting async-reactor no-op ("the op would block — do nothing and
/// await readability"), e.g. `Err(e) if e.kind() == io::ErrorKind::WouldBlock`.
/// Anchored on the `WouldBlock` path in the guard subtree (an `identifier`
/// node), covering `ErrorKind::WouldBlock`, `io::ErrorKind::WouldBlock`,
/// `std::io::ErrorKind::WouldBlock`, and a bare `WouldBlock` brought in by
/// `use`. An arm with no such guard is not affected and still flags.
fn guard_is_self_documenting(pattern: tree_sitter::Node, source: &[u8]) -> bool {
    pattern
        .child_by_field_name("condition")
        .is_some_and(|guard| subtree_references_would_block(guard, source))
}

fn subtree_references_would_block(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() == "identifier"
        && node.utf8_text(source).is_ok_and(|name| name == "WouldBlock")
    {
        return true;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(|child| subtree_references_would_block(child, source))
}

fn pattern_needs_justification(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "_" | "wildcard_pattern" => false,
        "match_pattern" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if pattern_needs_justification(child, source) {
                    return true;
                }
            }
            false
        }
        "or_pattern" => {
            // An or-pattern enumerating multiple explicit, non-wildcard
            // alternatives (`A | B | StmtKind::Err(_) => {}`) is self-documenting:
            // listing every case by name proves the author considered each one,
            // so an empty body is correct by construction and a comment would only
            // restate the pattern list. The error-swallowing heuristic (applied to
            // a standalone arm) is deliberately not run on the sub-patterns here.
            // The exception is a catch-all alternative (`A | _ => {}`,
            // `A | other => {}`) that hides the remaining cases — that still flags.
            if or_pattern_has_wildcard(node, source) {
                return true;
            }
            false
        }
        "tuple_struct_pattern" => {
            let Ok(text) = node.utf8_text(source) else {
                return false;
            };
            if !(text.starts_with("Err(") || text.contains("::Err(")) {
                return false;
            }
            // `Err(Self::REGISTERED) => {}` / `Err(MAX_RETRIES) => {}` pins the arm
            // to one specific known value and binds nothing — the self-documenting
            // lock-free CAS "already in this exact state" no-op, exactly like the
            // `Progress::None => {}` named-variant arm exempted above.
            if tuple_struct_pattern_binds_const(node, source) {
                return false;
            }
            !all_bindings_underscore_prefixed(node, source)
        }
        _ => false,
    }
}

/// True when an `or_pattern` includes a catch-all alternative — a bare wildcard
/// (`A | _ => {}`) or a rest pattern (`A | .. => {}`). Such an alternative hides
/// which remaining cases fall into the empty arm, so the explicit-enumeration
/// justification no longer holds and the arm still needs a comment.
///
/// A bare `identifier` alternative is a catch-all only when it is a FRESH BINDING
/// (`A | other => {}`): a lowercase-leading or `_`-prefixed name in pattern
/// position matches everything, exactly like `_`. An uppercase-leading bare
/// `identifier` (`None | Break => {}`) is a unit-variant or const reference, not
/// a binding — it names one specific case and keeps the enumeration explicit, so
/// it does not make the arm a catch-all. tree-sitter-rust parses both as
/// `identifier`; the leading-case convention is the only structural signal.
///
/// A `ref`/`mut` binding (`A | ref y => {}`, `A | mut z => {}`, parsed as
/// `ref_pattern` / `mut_pattern`) and an `@`-capture over a wildcard
/// (`A | y @ _ => {}`, parsed as `captured_pattern`) always capture whatever is
/// left, so they are catch-alls regardless of the binding name.
fn or_pattern_has_wildcard(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor).any(|child| match child.kind() {
        "_" | "wildcard_pattern" | "remaining_field_pattern" | "ref_pattern"
        | "mut_pattern" | "captured_pattern" => true,
        "identifier" => child
            .utf8_text(source)
            .is_ok_and(|name| name.starts_with('_') || name.starts_with(char::is_lowercase)),
        _ => false,
    })
}

/// True when the pattern introduces at least one variable binding and every
/// such binding is underscore-prefixed (Rust's signal for intentional
/// discard, e.g. `Err(_frame)`). Bindings are the `identifier` children of
/// the pattern other than the constructor path (`type` field of a
/// `tuple_struct_pattern`). A pattern with no bindings — e.g. `Err(_)` whose
/// `_` is a bare wildcard, not a binding — returns false.
fn all_bindings_underscore_prefixed(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut binding_count = 0usize;
    let mut cursor = node.walk();
    for (i, child) in node.children(&mut cursor).enumerate() {
        if child.kind() != "identifier" {
            continue;
        }
        if node.field_name_for_child(i as u32) == Some("type") {
            continue;
        }
        binding_count += 1;
        if !child.utf8_text(source).is_ok_and(|name| name.starts_with('_')) {
            return false;
        }
    }
    binding_count > 0
}

/// True when a `line_comment`/`block_comment` sits on the line(s) immediately
/// preceding the loop, with no blank line between the comment and the loop. The
/// loop expression is usually wrapped in an `expression_statement`, so the
/// comment is a sibling of that statement; for a tail-position loop the comment
/// is a sibling of the loop itself. Such a comment justifies an empty body the
/// same way an inside-the-block comment does — it explains the intentional
/// inaction (e.g. the iterator-drain pattern where work happens in the
/// condition). A blank line between the two breaks the association.
fn has_leading_comment(node: tree_sitter::Node, source: &[u8]) -> bool {
    let anchor = match node.parent() {
        Some(parent) if parent.kind() == "expression_statement" => parent,
        _ => node,
    };
    let Some(prev) = anchor.prev_sibling() else {
        return false;
    };
    if prev.kind() != "line_comment" && prev.kind() != "block_comment" {
        return false;
    }
    let gap = source
        .get(prev.end_byte()..anchor.start_byte())
        .unwrap_or_default();
    gap.iter().filter(|&&b| b == b'\n').count() <= 1
}

/// True when the subtree rooted at `node` contains a `call_expression`. Used on
/// a `while` condition: a call there (`!peripheral.ready()`, `reg.read().bit()`,
/// `set.join_next().await.is_some()`) makes the condition self-documenting, so
/// the empty body is the standard polling / iterator-drain idiom and needs no
/// body comment. Negations (`unary_expression`) and comparisons
/// (`binary_expression`) wrapping the call are walked through.
fn contains_call(node: tree_sitter::Node) -> bool {
    if node.kind() == "call_expression" {
        return true;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor).any(contains_call)
}

/// True when a `for`-loop binds its item to a bare wildcard (`for _ in expr {}`).
/// Draining an iterator for its side effects — triggering `Drop`, or advancing a
/// `by_ref()` cursor without consuming the outer iterator (`for _ in it.by_ref()
/// .take(n) {}`) — is the idiomatic use of such a loop: the `_` states the yielded
/// value is intentionally ignored and the iteration itself is the goal, so the
/// empty body is self-documenting, exactly like a wildcard `match` arm. A named
/// binding (`for x in expr {}`) or a destructuring pattern (`for (k, v) in map {}`)
/// is not a bare wildcard and still flags. Non-`for` loops have no `pattern` field
/// and return false.
fn for_pattern_is_wildcard(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(pattern) = node.child_by_field_name("pattern") else {
        return false;
    };
    matches!(pattern.kind(), "_" | "wildcard_pattern")
        || (pattern.kind() == "identifier"
            && pattern.utf8_text(source).is_ok_and(|name| name == "_"))
}

fn loop_name(kind: &str) -> &'static str {
    match kind {
        "for_expression" => "for",
        "while_expression" => "while",
        "loop_expression" => "loop",
        _ => "loop",
    }
}

fn flag_empty(
    container: tree_sitter::Node,
    body: tree_sitter::Node,
    what: &str,
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !block_is_empty(body) {
        return;
    }
    let pos = container.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "justify-inaction".into(),
        message: format!(
            "Empty `{what}` block \u{2014} add a comment inside explaining why the inaction is intentional."
        ),
        severity: Severity::Error,
        span: None,
    });
}

crate::ast_check! { on ["if_expression", "else_clause", "match_arm", "for_expression", "while_expression", "loop_expression"] => |node, _source, ctx, diagnostics|
match node.kind() {
        "if_expression" => {
            if let Some(cons) = node.child_by_field_name("consequence") {
                flag_empty(node, cons, "if", ctx, diagnostics);
            }
        }
        "else_clause" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if child.kind() == "block" {
                    flag_empty(node, child, "else", ctx, diagnostics);
                    break;
                }
            }
        }
        "match_arm" => {
            if let Some(value) = node.child_by_field_name("value")
                && block_is_empty(value) && match_arm_needs_justification(node, _source) {
                    flag_empty(node, value, "match arm", ctx, diagnostics);
                }
        }
        "for_expression" | "while_expression" | "loop_expression" => {
            if let Some(body) = node.child_by_field_name("body")
                && !has_leading_comment(node, _source)
                && !for_pattern_is_wildcard(node, _source)
                && !(node.kind() == "while_expression"
                    && node
                        .child_by_field_name("condition")
                        .is_some_and(contains_call)) {
                    flag_empty(node, body, loop_name(node.kind()), ctx, diagnostics);
                }
        }
        _ => {}
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    // -- if / else --

    #[test]
    fn flags_empty_if() {
        assert_eq!(run_on("fn f(x: bool) { if x {} }").len(), 1);
    }

    #[test]
    fn flags_empty_else() {
        let src = "fn f(x: bool) { if x { go(); } else {} }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_if_with_comment_inside() {
        let src = "fn f(x: bool) { if x { /* handled upstream */ } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_else_with_line_comment_inside() {
        let src = "fn f(x: bool) {\n    if x {\n        go();\n    } else {\n        // intentional no-op\n    }\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_non_empty_else() {
        let src = "fn f(x: bool) { if x { a(); } else { b(); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_else_if_chain() {
        let src = "fn f(x: i32) { if x == 1 { a(); } else if x == 2 { b(); } }";
        assert!(run_on(src).is_empty());
    }

    // -- match arms --

    #[test]
    fn allows_empty_named_variant_arm() {
        let src = "fn f(x: Option<u8>) { match x { Some(v) => go(v), None => {} } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_empty_scoped_variant_arm() {
        let src = "fn f(x: u8) { match x { Progress::Active(v) => go(v), Progress::None => {} } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_empty_literal_arm() {
        let src = "fn f(x: u8) { match x { 0 => {}, 1 => go() } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_empty_err_arm() {
        let src = "fn f(r: Result<u8, E>) { match r { Ok(v) => go(v), Err(_) => {} } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_empty_wildcard_arm() {
        let src = "fn f(x: u8) { match x { 0 => go(), _ => {} } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_empty_err_arm_with_underscore_binding_issue_1444() {
        let src = "fn f(r: Result<u8, E>) { match r { Ok(v) => go(v), Err(_frame) => {} } }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_empty_or_arm_all_underscore_bindings() {
        let src = "fn f(r: Result<u8, E>) { match r { Ok(_a) | Err(_b) => {} } }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_empty_explicit_multi_variant_or_arm_issue_5643() {
        // An or-pattern enumerating every leaf variant is self-documenting —
        // listing each case by name is stronger than a wildcard `_ => {}`. The
        // `StmtKind::Err(_)` variant here is a domain enum variant, not the std
        // `Result::Err`, and the explicit enumeration documents the no-op.
        let src = "fn f(stmt: S) { match stmt.kind { \
                   StmtKind::Emit(expr) => walk(expr), \
                   StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue | StmtKind::Err(_) => {} \
                   } }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_empty_two_variant_or_arm_issue_5643() {
        let src = "fn f(x: E) { match x { E::Used(v) => go(v), E::A | E::B => {} } }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_empty_or_arm_bare_unqualified_variants_issue_5643() {
        // Bare unqualified variant names (`None | Break`) parse as `identifier`,
        // same node kind as a binding. The uppercase-leading convention marks them
        // as variant references — explicit enumeration, not a catch-all binding.
        let src = "fn f(x: E) { match x { Used(v) => go(v), None | Break => {} } }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn flags_empty_or_arm_with_wildcard_issue_5643() {
        // `A | _ => {}` is a catch-all: the `_` hides the remaining cases, so the
        // explicit-enumeration justification no longer holds.
        let src = "fn f(x: E) { match x { E::Used(v) => go(v), E::A | _ => {} } }";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    #[test]
    fn flags_empty_or_arm_with_binding_catch_all_issue_5643() {
        // `A | other => {}` — a bare binding alternative matches everything, like `_`.
        let src = "fn f(x: E) { match x { E::Used(v) => go(v), E::A | other => {} } }";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    #[test]
    fn flags_empty_or_arm_with_ref_mut_binding_catch_all_issue_5643() {
        // `A | ref y => {}` / `A | mut z => {}` — a ref/mut binding always captures
        // the rest, so it is a catch-all that hides the remaining cases.
        let with_ref = "fn f(x: E) { match x { E::Used(v) => go(v), E::A | ref y => {} } }";
        let with_mut = "fn f(x: E) { match x { E::Used(v) => go(v), E::A | mut z => {} } }";
        assert_eq!(run_on(with_ref).len(), 1, "{:?}", run_on(with_ref));
        assert_eq!(run_on(with_mut).len(), 1, "{:?}", run_on(with_mut));
    }

    #[test]
    fn flags_empty_or_arm_with_at_wildcard_capture_catch_all_issue_5643() {
        // `A | y @ _ => {}` — an `@`-capture over a wildcard matches everything.
        let src = "fn f(x: E) { match x { E::Used(v) => go(v), E::A | y @ _ => {} } }";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    #[test]
    fn flags_empty_err_arm_with_named_binding() {
        let src = "fn f(r: Result<u8, E>) { match r { Ok(v) => go(v), Err(e) => {} } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_empty_err_arm_with_named_frame_binding() {
        let src = "fn f(r: Result<u8, E>) { match r { Ok(v) => go(v), Err(frame) => {} } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_empty_scoped_err_arm_with_named_binding() {
        let src = "fn f(r: Result<u8, E>) { match r { Ok(v) => go(v), my::Err(e) => {} } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_empty_err_arm_scoped_const_issue_3986() {
        // Lock-free CAS idiom: `Err(Self::REGISTERED) => {}` pins the arm to
        // one specific const value and binds nothing — self-documenting no-op.
        let src = "fn f(r: Result<u8, E>) { match r { \
                   Ok(v) => go(v), \
                   Err(Self::REGISTERED) => {} \
                   Err(_state) => { other(); } \
                   } }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_empty_err_arm_screaming_snake_const_issue_3986() {
        let src = "fn f(r: Result<u8, E>) { match r { \
                   Ok(v) => go(v), \
                   Err(MAX_RETRIES) => {} \
                   Err(_state) => { other(); } \
                   } }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn flags_empty_err_arm_lowercase_binding_issue_3986() {
        // Narrowness guard: a lowercase identifier is a FRESH BINDING, not a
        // const — `Err(frame) => {}` still needs justification and must fire.
        let src = "fn f(r: Result<u8, E>) { match r { Ok(v) => go(v), Err(frame) => {} } }";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    #[test]
    fn allows_would_block_guard_arm_issue_5361() {
        // Canonical async non-blocking I/O reactor pattern: the guard
        // `e.kind() == io::ErrorKind::WouldBlock` documents the empty body.
        let src = "fn f(r: Result<u8, E>) { match r { \
                   Err(e) if e.kind() == io::ErrorKind::WouldBlock => {} \
                   res => return res, \
                   } }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_would_block_guard_arm_bare_path_issue_5361() {
        let src = "fn f(r: Result<u8, E>) { match r { \
                   Err(e) if e.kind() == ErrorKind::WouldBlock => {} \
                   res => return res, \
                   } }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_would_block_guard_arm_bare_ident_issue_5361() {
        // `use std::io::ErrorKind::WouldBlock;` brings in the bare identifier.
        let src = "fn f(r: Result<u8, E>) { match r { \
                   Err(e) if e.kind() == WouldBlock => {} \
                   res => return res, \
                   } }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn flags_empty_err_arm_with_unrelated_guard_issue_5361() {
        // Narrowness guard: a guard that does NOT name a recognized do-nothing
        // I/O condition leaves the empty arm unjustified. The sibling is a
        // non-diverging call so this isolates the guard concern from the
        // sibling-divergence exemption (issue #7495).
        let src = "fn f(r: Result<u8, E>) { match r { \
                   Err(e) if e.is_timeout() => {} \
                   res => handle(res), \
                   } }";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    #[test]
    fn allows_empty_wildcard_arm_issue_1002() {
        let src = "fn f(v: E) { match v { E::Specific(fld) => { go(fld); } _ => {} } }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_unit_wildcard_arm() {
        let src = "fn f(x: u8) { match x { 0 => go(), _ => () } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_match_arm_with_comment_inside() {
        let src = r#"
fn f(x: Option<u8>) {
    match x {
        Some(v) => go(v),
        None => {
            // already handled upstream
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_non_empty_match_arm() {
        let src = "fn f(x: u8) { match x { 0 => {}, _ => go() } }";
        assert!(run_on(src).is_empty());
    }

    // -- sibling-arm divergence: match-as-assertion (issue #7495) --

    #[test]
    fn allows_empty_arm_when_sibling_panics_issue_7495() {
        // The reported false positive: an assertion-style `match` where the empty
        // arm is the expected case and the sibling `_ => panic!(…)` documents that
        // any other variant aborts.
        let src = "fn f(r: Result<u8, E>) { match r { \
                   Err(Error::IO { .. }) => {} \
                   _ => panic!(\"Expected Err with IO variant\"), \
                   } }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_empty_arm_when_sibling_unreachable_issue_7495() {
        let src = "fn f(r: Result<u8, E>) { match r { Err(e) => {} _ => unreachable!(\"nope\"), } }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_empty_arm_when_sibling_todo_issue_7495() {
        let src = "fn f(r: Result<u8, E>) { match r { Err(e) => {} _ => todo!(), } }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_empty_arm_when_sibling_returns_issue_7495() {
        let src = "fn f(r: Result<u8, E>) { match r { Err(e) => {} _ => return, } }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_empty_arm_when_sibling_breaks_issue_7495() {
        let src = "fn f(r: Result<u8, E>) { loop { match r { Err(e) => {} _ => break, } } }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_empty_arm_when_sibling_continues_issue_7495() {
        let src = "fn f(r: Result<u8, E>) { loop { match r { Err(e) => {} _ => continue, } } }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_empty_arm_when_sibling_braced_panic_issue_7495() {
        // Braced diverging body: the block's tail expression is the panic macro.
        let src = "fn f(r: Result<u8, E>) { match r { Err(e) => {} _ => { panic!(\"x\") } } }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_empty_arm_when_sibling_braced_return_issue_7495() {
        // Braced diverging body with a trailing `;`: the tail is an
        // `expression_statement` wrapping the `return`.
        let src = "fn f(r: Result<u8, E>) { match r { Err(e) => {} _ => { return; } } }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn flags_empty_arm_when_no_sibling_diverges_issue_7495() {
        // No sibling diverges: a plain call arm must NOT be read as divergence, so
        // the uncommented empty arm still fires.
        let src = "fn f(r: Result<u8, E>) { match r { Ok(v) => go(v), Err(e) => {} } }";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    #[test]
    fn flags_empty_arm_when_sibling_block_does_not_diverge_issue_7495() {
        // A sibling block whose tail is a plain call does not diverge — the empty
        // arm is not exempted.
        let src = "fn f(r: Result<u8, E>) { match r { Err(e) => {} _ => { go(); } } }";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    #[test]
    fn allows_empty_arm_when_diverging_sibling_is_binding_catch_all_issue_7495() {
        // A bare lowercase binding (`res`) is a catch-all, exactly like `_`, so a
        // diverging one still makes the match an assertion.
        let src = "fn f(r: Result<u8, E>) { match r { Err(e) => {} res => return res, } }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn flags_empty_arm_when_diverging_sibling_is_specific_variant_issue_7495() {
        // The diverging arm is a SPECIFIC variant (`Ok(v)`), not a catch-all, so
        // the match is not an assertion: the empty `Err` arm still silently
        // swallows and must flag.
        let src = "fn f(r: Result<u8, E>) { match r { Ok(v) => return process(v), Err(e) => {} } }";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    #[test]
    fn flags_empty_arm_when_diverging_catch_all_is_guarded_issue_7495() {
        // A guarded catch-all can fail its guard, so it is not an unconditional
        // catch-all — the empty arm is not exempted.
        let src = "fn f(r: Result<u8, E>) { match r { Err(e) => {} other if other.is_x() => panic!(), } }";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    // -- loops --

    #[test]
    fn flags_empty_while() {
        assert_eq!(run_on("fn f(running: bool) { while running {} }").len(), 1);
    }

    #[test]
    fn flags_empty_for() {
        assert_eq!(run_on("fn f(xs: &[u8]) { for x in xs {} }").len(), 1);
    }

    // -- bare-wildcard for-loop drain (issue #6322) --

    #[test]
    fn allows_empty_for_wildcard_drain_issue_6322() {
        let src = "fn f(half: &mut std::vec::IntoIter<u8>) { for _ in half.by_ref() {} }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_empty_for_wildcard_byref_take_issue_6322() {
        let src = "fn f(iter: &mut std::vec::IntoIter<u8>) { for _ in iter.by_ref().take(3) {} }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn flags_empty_for_underscore_prefixed_binding_issue_6322() {
        // `_x` is a named binding, not the bare wildcard `_`: it still flags,
        // matching how the rule treats top-level match-arm patterns.
        assert_eq!(run_on("fn f(xs: &[u8]) { for _x in xs {} }").len(), 1);
    }

    #[test]
    fn flags_empty_for_destructuring_binding_issue_6322() {
        let src = "fn f(map: std::collections::HashMap<u8, u8>) { for (k, v) in map {} }";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    #[test]
    fn flags_empty_loop() {
        assert_eq!(run_on("fn f() { loop {} }").len(), 1);
    }

    #[test]
    fn allows_while_with_comment() {
        let src = "fn f() { while poll() { /* busy-wait for the device */ } }";
        assert!(run_on(src).is_empty());
    }

    // -- scope exclusions --

    #[test]
    fn does_not_flag_empty_fn_body() {
        assert!(run_on("fn stub() {}").is_empty());
    }

    #[test]
    fn does_not_flag_empty_closure_body() {
        let src = "fn f() { let cb = || {}; cb(); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_unit_match_arm() {
        let src = "fn f(x: Option<u8>) { match x { Some(v) => go(v), None => () } }";
        assert!(run_on(src).is_empty());
    }

    // -- comment immediately above the loop (issue #2200) --

    #[test]
    fn allows_while_with_leading_comment_issue_2200() {
        let src = "fn f() {\n    // Eager version of `skip_while`.\n    while it.next_if(|x| x.is_ws()).is_some() {}\n}\n";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_while_with_leading_block_comment() {
        let src = "fn f() {\n    /* drain whitespace */\n    while it.next_if(|x| x.is_ws()).is_some() {}\n}\n";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_for_with_leading_comment() {
        let src = "fn f(xs: &[u8]) {\n    // side effects happen in the iterator\n    for _ in xs.iter().inspect(|_| log()) {}\n}\n";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_loop_with_leading_comment() {
        let src = "fn f() {\n    // spin forever on purpose\n    loop {}\n}\n";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn flags_while_with_no_comment_anywhere() {
        let src = "fn f(running: bool) {\n    let x = 1;\n    while running {}\n}\n";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    #[test]
    fn flags_while_with_comment_separated_by_blank_line() {
        let src = "fn f(running: bool) {\n    // unrelated note about something else\n\n    while running {}\n}\n";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    // -- call-in-condition exemption (issue #1436) --

    #[test]
    fn allows_empty_while_polling_register_negated_call_issue_1436() {
        let src = "fn f() { while !RCC.cr().read().hsirdy() {} }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_empty_while_polling_register_compared_call_issue_1436() {
        let src = "fn f() { while RCC.cfgr().read().sws() != Sysclk::Hsi {} }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_empty_while_join_next_await_call_issue_1436() {
        let src = "async fn f() { while clients.join_next().await.is_some() {} }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_empty_while_plain_call_issue_1436() {
        let src = "fn f() { while poll() {} }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn flags_empty_while_bare_flag_no_call_issue_1436() {
        let src = "fn f(running: bool) { while running {} }";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    #[test]
    fn flags_empty_while_literal_no_call_issue_1436() {
        assert_eq!(run_on("fn f() { while true {} }").len(), 1);
    }

    #[test]
    fn flags_empty_while_comparison_no_call_issue_1436() {
        let src = "fn f(x: i32, n: i32) { while x < n {} }";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }
}
