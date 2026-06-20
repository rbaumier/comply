//! no-empty-catch Rust backend — flag silently-ignored errors.
//!
//! Rust has no `catch`, but the equivalent error-swallowing patterns are:
//!
//! - `match r { Ok(_) => ..., Err(_) => {} }` — empty `Err(_)` arm.
//! - `if let Err(_) = r {}` — empty if-let block over an `Err(_)` pattern.
//!
//! A body is considered "empty" when it is a `block` with zero named
//! children AND contains no comment. A comment acts as an explicit
//! justification for swallowing the error, whether placed inside the `{}`
//! block or as a leading comment on its own line directly above the arm.
//!
//! An empty `Err(CONST_PATH) => {}` arm is exempt: a payload that is a
//! const/path binding nothing (`Err(Self::REGISTERED)`, `Err(MAX_RETRIES)`)
//! pins the arm to one specific known error value — the lock-free CAS
//! "already in this exact state, nothing to do" no-op, not a swallow. A
//! wildcard (`Err(_)`) or fresh binding (`Err(e)`) stays flagged.
//!
//! A *guarded* empty `Err` arm (`Err(ref e) if e.kind() == Interrupted => {}`)
//! is also exempt: the guard isolates one specific condition to no-op (the
//! EINTR retry-on-interrupt idiom), and `match` exhaustiveness forces every
//! other error into a sibling arm. The guard documents intent rather than
//! silently swallowing the error.
//!
//! An empty `Err` arm is also exempt when it is one half of a *disjoint* error
//! partition whose other half — a sibling `Err(...)` arm with a non-empty body —
//! owns the error. The partition is disjoint when EITHER the capturing sibling
//! is guarded (`Err(e) if cond => capture`, the First-combinator "keep first
//! error" idiom, where the bare empty arm is the complementary remainder) OR
//! this empty arm carries a structured discriminant (`Err((false, _))`,
//! `Err(Variant(..))`, the fatal/non-fatal split). A bare `Err(_)`/`Err(e)` arm
//! beside an unguarded capturing sibling still swallows every other error class
//! and stays flagged; a lone empty `Err` arm likewise stays flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{arm_body_is_diverging, tuple_struct_pattern_binds_const};

crate::ast_check! { on ["match_arm", "let_condition", "let_chain", "if_expression"] => |node, source, ctx, diagnostics|
match node.kind() {
        "match_arm" => {
            let Some(pattern) = node.child_by_field_name("pattern") else { return };
            if !pattern_is_err(&pattern, source) {
                return;
            }
            let Some(value) = node.child_by_field_name("value") else { return };
            if !is_empty_block(&value, source) {
                return;
            }
            // A value-specific no-op: `Err(Self::REGISTERED) => {}` /
            // `Err(MAX_RETRIES) => {}` matches one specific const error
            // value and binds nothing — the lock-free CAS "already in this
            // exact state, nothing to do" arm, not silent error-swallowing.
            if pattern_is_const_err(&pattern, source) {
                return;
            }
            // A guarded empty arm (`Err(e) if <cond> => {}`) intentionally
            // no-ops one specific condition — the EINTR retry idiom
            // `Err(ref e) if e.kind() == ErrorKind::Interrupted => {}`. Match
            // exhaustiveness forces every other error into a sibling arm, so the
            // guard documents intent rather than silently swallowing the error.
            // The `pattern` field is a `match_pattern` = seq(_pattern,
            // optional("if" condition)); the guard is its `condition` child.
            if pattern.child_by_field_name("condition").is_some() {
                return;
            }
            // A controlled assertion: `Err(Foo) => {}` paired with a
            // sibling arm that diverges (`v => panic!(...)`,
            // `_ => unreachable!()`, `_ => return Err(e)`, …) asserts the
            // result must be this exact error — the empty arm is the
            // success case, not silent error-swallowing.
            if has_diverging_sibling_arm(&node, source) {
                return;
            }
            // A first-match / non-fatal partition: an empty `Err` arm whose
            // error is provably owned by a *disjoint* sibling `Err(...)` arm
            // that captures it (non-empty body). Two disjoint shapes qualify:
            //   - The First combinator: a GUARDED capturing sibling
            //     (`Err(err) if first_err.is_none() => first_err = Some(err)`)
            //     keeps only the first error; the bare `Err(_) => {}` arm is
            //     the complementary "already captured one, skip" remainder.
            //   - The fatal/non-fatal split: a STRUCTURED discriminant on this
            //     arm (`Err((false, _err)) => {}`) targets a specific non-fatal
            //     subset while the sibling `Err((true, err)) => ret_error = err`
            //     owns the fatal one.
            // A bare `Err(_)`/`Err(e)` arm beside an UNGUARDED capturing sibling
            // (`Err(Specific(e)) => handle(e)`) still swallows every other error
            // and stays flagged — that partition is not disjoint.
            if has_disjoint_capturing_sibling_err_arm(&node, &pattern, source) {
                return;
            }
            // An explicit justification placed as a leading comment directly
            // above the arm (`// why\nErr(_) => {}`, the idiomatic Rust
            // placement) is the same escape hatch as an in-brace comment.
            if arm_has_leading_comment(&node) {
                return;
            }
            push_diag(node, ctx, diagnostics);
        }
        "let_condition" | "let_chain" => {
            // `if let Err(_) = x` is represented as `if_expression` whose
            // condition is a `let_condition`. We flag at the if-expression
            // level instead — handled below.
        }
        "if_expression" => {
            let Some(cond) = node.child_by_field_name("condition") else { return };
            if !if_let_is_err(&cond, source) {
                return;
            }
            let Some(cons) = node.child_by_field_name("consequence") else { return };
            if !is_empty_block(&cons, source) {
                return;
            }
            push_diag(node, ctx, diagnostics);
        }
        _ => {}
    }
}

fn push_diag(
    node: tree_sitter::Node,
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-empty-catch".into(),
        message: "Empty error-handling block silently swallows the error \u{2014} \
                  log it, propagate it, or add a comment explaining why."
            .into(),
        severity: Severity::Warning,
        span: None,
    });
}

/// True if any *other* arm of the same `match` diverges (panics, aborts,
/// or returns an error). The empty `Err(...)` arm's parent is the
/// `match_block`; we scan its `match_arm` siblings, skipping the arm
/// itself.
fn has_diverging_sibling_arm(arm: &tree_sitter::Node, source: &[u8]) -> bool {
    let Some(match_block) = arm.parent() else {
        return false;
    };
    if match_block.kind() != "match_block" {
        return false;
    }
    let mut cursor = match_block.walk();
    for sibling in match_block.named_children(&mut cursor) {
        if sibling.kind() != "match_arm" || sibling.id() == arm.id() {
            continue;
        }
        if arm_body_is_diverging(sibling, source) {
            return true;
        }
    }
    false
}

/// True if the empty `Err` `arm` is one half of a *disjoint* error partition
/// whose other half — a sibling `Err(...)` arm with a non-empty body — owns the
/// error. The partition is disjoint (so this arm provably drops nothing the
/// sibling needed to see) when EITHER:
///
/// - the capturing sibling is **guarded** (`Err(e) if cond => capture`): the
///   bare empty arm is only reached once the guard has stopped capturing, so it
///   is the complementary remainder (the First-combinator "keep first error"
///   idiom); OR
/// - this empty arm carries a **structured discriminant** (`Err((false, _))`,
///   `Err(Variant(..))`, or a literal `Err(1)`/`Err("eof")`): it targets one
///   specific error subset, leaving the sibling to own the rest (the
///   fatal/non-fatal split).
///
/// A bare `Err(_)`/`Err(e)` arm beside an UNGUARDED capturing sibling is NOT a
/// disjoint partition — it swallows every error class the sibling does not match
/// — so this returns false and the arm stays flagged.
fn has_disjoint_capturing_sibling_err_arm(
    arm: &tree_sitter::Node,
    arm_pattern: &tree_sitter::Node,
    source: &[u8],
) -> bool {
    let Some(match_block) = arm.parent() else {
        return false;
    };
    if match_block.kind() != "match_block" {
        return false;
    }
    let this_arm_is_structured = err_pattern_is_structured(arm_pattern);
    let mut cursor = match_block.walk();
    for sibling in match_block.named_children(&mut cursor) {
        if sibling.kind() != "match_arm" || sibling.id() == arm.id() {
            continue;
        }
        let Some(pattern) = sibling.child_by_field_name("pattern") else {
            continue;
        };
        if !pattern_is_err(&pattern, source) {
            continue;
        }
        let Some(value) = sibling.child_by_field_name("value") else {
            continue;
        };
        if is_empty_block(&value, source) {
            continue;
        }
        // Non-empty `Err(...)` sibling that captures the error. Disjoint when
        // the sibling is guarded or this arm is a structured discriminant.
        let sibling_is_guarded = pattern.child_by_field_name("condition").is_some();
        if sibling_is_guarded || this_arm_is_structured {
            return true;
        }
    }
    false
}

/// True if `pattern` (an `Err(...)` match pattern) destructures a *structured
/// discriminant* — its payload is a tuple/struct/enum-variant shape or a
/// literal, e.g. `Err((false, _err))` or `Err(Variant(..))`. A bare wildcard
/// (`Err(_)`) or a bare binding (`Err(e)`) is NOT structured: it matches every
/// error value, so it cannot be the disjoint half of a partition.
fn err_pattern_is_structured(pattern: &tree_sitter::Node) -> bool {
    let inner = unwrap_match_pattern(*pattern);
    if inner.kind() != "tuple_struct_pattern" {
        return false;
    }
    // The lone payload (skipping the `Err`/`Result::Err` constructor in the
    // `type` field). A bare `_`/binding payload is not a discriminant.
    let mut cursor = inner.walk();
    let payloads: Vec<tree_sitter::Node> = inner
        .children(&mut cursor)
        .enumerate()
        .filter(|(i, child)| {
            child.is_named() && inner.field_name_for_child(*i as u32) != Some("type")
        })
        .map(|(_, child)| child)
        .collect();
    let [payload] = payloads.as_slice() else {
        return false;
    };
    !matches!(payload.kind(), "_" | "identifier" | "mut_pattern" | "ref_pattern")
}

/// True if a `line_comment`/`block_comment` is the arm's immediate preceding
/// named sibling in the `match_block` AND sits on its own line above the arm —
/// the idiomatic Rust placement of a justification comment for an empty arm.
///
/// A comment is a named sibling of the `match_block` (same level as the arms),
/// not a child of the arm's `{}` body, so the in-brace `is_empty_block` check
/// never sees it; this mirrors that escape hatch for the leading placement.
///
/// Trailing-comment edge: a comment trailing the *previous* arm
/// (`Ok(v) => go(v), // ...\nErr(_) => {}`) is also the arm's preceding named
/// sibling. We attribute the comment to the arm it directly precedes by
/// requiring it to start on a row strictly below the node before it, so a
/// trailing comment of the previous arm does NOT justify this empty arm.
fn arm_has_leading_comment(arm: &tree_sitter::Node) -> bool {
    let Some(prev) = arm.prev_named_sibling() else {
        return false;
    };
    if prev.kind() != "line_comment" && prev.kind() != "block_comment" {
        return false;
    }
    // Reject a comment that trails the previous arm rather than leading this one.
    if let Some(before) = prev.prev_named_sibling()
        && prev.start_position().row <= before.end_position().row
    {
        return false;
    }
    true
}

fn is_empty_block(node: &tree_sitter::Node, _source: &[u8]) -> bool {
    if node.kind() != "block" {
        return false;
    }
    if node.named_child_count() != 0 {
        return false;
    }
    // A block with only a comment has zero named children in tree-sitter-rust,
    // but the comment is still reachable via the raw child list.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "line_comment" || child.kind() == "block_comment" {
            return false;
        }
    }
    true
}

/// True if `pattern` is `Err(_)` / `Err(..)` / `Err(e)` — an error-swallowing pattern.
///
/// The `match_arm` `pattern` field may be a `match_pattern` wrapper or the
/// pattern node directly, depending on grammar version. We descend through
/// `match_pattern` wrappers and inspect the textual form of the pattern,
/// which reliably matches `Err(...)` and qualified forms like `Result::Err(...)`.
fn pattern_is_err(node: &tree_sitter::Node, source: &[u8]) -> bool {
    let inner = unwrap_match_pattern(*node);
    let text = inner.utf8_text(source).unwrap_or("").trim();
    // Match `Err(...)` or `<path>::Err(...)`.
    if let Some(paren) = text.find('(') {
        let head = text[..paren].trim();
        return head == "Err" || head.ends_with("::Err");
    }
    false
}

/// True if `node` is `Err(CONST_PATH)` — an `Err(...)` whose payload is a
/// const/path pattern that binds nothing (`Err(Self::REGISTERED)`,
/// `Err(MAX_RETRIES)`). Such an arm pins itself to one specific known error
/// value, so an empty body is a deliberate value-specific no-op, not a swallow.
/// A wildcard (`Err(_)`) or a fresh binding (`Err(e)`) is NOT a const pattern
/// and stays flagged.
fn pattern_is_const_err(node: &tree_sitter::Node, source: &[u8]) -> bool {
    let inner = unwrap_match_pattern(*node);
    inner.kind() == "tuple_struct_pattern" && tuple_struct_pattern_binds_const(inner, source)
}

fn unwrap_match_pattern(node: tree_sitter::Node) -> tree_sitter::Node {
    if node.kind() == "match_pattern"
        && let Some(inner) = node.named_child(0)
    {
        return inner;
    }
    node
}

/// True if `cond` is a `let_condition` of the shape `let Err(_) = expr`.
fn if_let_is_err(cond: &tree_sitter::Node, source: &[u8]) -> bool {
    if cond.kind() != "let_condition" {
        return false;
    }
    let Some(pattern) = cond.child_by_field_name("pattern") else {
        return false;
    };
    pattern_is_err(&pattern, source)
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
    fn flags_empty_err_match_arm() {
        let src = "fn f(r: Result<u8, E>) { match r { Ok(v) => go(v), Err(_) => {} } }";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("swallows"));
    }

    #[test]
    fn flags_empty_if_let_err() {
        let src = "fn f(r: Result<u8, E>) { if let Err(_) = r {} }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_non_empty_err_match_arm() {
        let src = "fn f(r: Result<u8, E>) { match r { Ok(v) => go(v), Err(e) => log(e) } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_err_match_arm_with_comment() {
        let src = "fn f(r: Result<u8, E>) { match r { Ok(v) => go(v), Err(_) => { /* ignored on purpose */ } } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_empty_ok_match_arm() {
        // Not an error-swallowing pattern — out of scope for this rule.
        let src = "fn f(r: Result<u8, E>) { match r { Ok(_) => {}, Err(e) => log(e) } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_non_empty_if_let_err() {
        let src = "fn f(r: Result<u8, E>) { if let Err(e) = r { log(e); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_empty_err_arm_with_panicking_sibling() {
        // Issue #1504: `Err(DeserializationError(_)) => {}` paired with a
        // `v => panic!(...)` arm is a controlled assertion that the result
        // must be exactly this error — the empty arm is the success case.
        let src = "fn f(values: Result<u8, E>) { match values { \
                   Err(DeserializationError(_)) => {} \
                   v => panic!(\"Expected a deserialization error, got {:?}\", v), \
                   } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_empty_err_arm_with_unreachable_sibling() {
        let src = "fn f(r: Result<u8, E>) { match r { \
                   Err(_) => {} \
                   _ => unreachable!(), \
                   } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_empty_err_arm_without_diverging_sibling() {
        // Negative space: a genuinely empty error arm with no diverging
        // sibling is still silent error-swallowing and must fire.
        let src = "fn f(r: Result<u8, E>) { match r { \
                   Ok(v) => go(v), \
                   Err(_) => {} \
                   } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_empty_err_arm_scoped_const_issue_3986() {
        // Lock-free CAS idiom: `Err(Self::REGISTERED) => {}` matches one
        // specific const value and binds nothing — the "already done" no-op.
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
        // const — `Err(frame) => {}` still swallows the error and must fire.
        let src = "fn f(r: Result<u8, E>) { match r { \
                   Ok(v) => go(v), \
                   Err(frame) => {} \
                   } }";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    #[test]
    fn flags_empty_err_arm_underscore_binding_issue_3986() {
        // Narrowness guard: an underscore-prefixed binding is still a binding,
        // not a const — `Err(_state) => {}` with an empty body still swallows.
        let src = "fn f(r: Result<u8, E>) { match r { \
                   Ok(v) => go(v), \
                   Err(_state) => {} \
                   } }";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    #[test]
    fn allows_err_arm_with_leading_comment_issue_3988() {
        // Issue #3988: the justification is placed as a leading comment on its
        // own line directly above the empty `Err(_) => {}` arm — the idiomatic
        // Rust placement, not between the braces.
        let src = "fn f(r: Result<u8, E>) { match r { \
                   Ok(v) => go(v),\n\
                   // documented: safe to ignore here\n\
                   Err(_) => {} \
                   } }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_err_arm_with_leading_block_comment_issue_3988() {
        let src = "fn f(r: Result<u8, E>) { match r { \
                   Ok(v) => go(v),\n\
                   /* documented: safe to ignore here */\n\
                   Err(_) => {} \
                   } }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_guarded_empty_err_arm_eintr_retry_issue_4476() {
        // Issue #4476: the EINTR retry idiom — a guarded empty `Err` arm
        // (`Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}`) no-ops
        // one specific condition (retry via the surrounding loop). Match
        // exhaustiveness forces every other error into the sibling `Err(e)` arm.
        let src = "fn f(writer: &mut W, buf: &[u8]) {\n\
                   loop {\n\
                   match writer.write(buf) {\n\
                   Ok(n) => { let _ = n; }\n\
                   Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}\n\
                   Err(e) => { let _ = e; break; }\n\
                   }\n\
                   }\n\
                   }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn flags_unguarded_empty_err_arm_wildcard_issue_4476() {
        // Narrowness guard: an UNGUARDED empty `Err(_)` catch-all still
        // silently swallows the error and must fire — only the guard exempts.
        let src = "fn f(r: Result<u8, E>) { match r { Ok(_) => {}, Err(_) => {} } }";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    #[test]
    fn flags_unguarded_empty_err_arm_binding_issue_4476() {
        // Narrowness guard: an UNGUARDED empty `Err(e)` arm still swallows.
        let src = "fn f(r: Result<u8, E>) { match r { Ok(v) => g(v), Err(e) => {} } }";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    #[test]
    fn allows_empty_err_arm_in_first_match_combinator_issue_5000() {
        // Issue #5000: the `First` combinator loop tries each candidate; a
        // GUARDED sibling `Err(err) if first_err.is_none()` arm captures the
        // first error, and the empty `Err(_) => {}` arm is the complementary
        // remainder discarding redundant subsequent errors. The guard makes the
        // partition disjoint — the empty arm provably drops nothing the sibling
        // needed to see.
        let src = "fn f(items: &[I]) -> Result<u8, E> { \
                   let mut first_err = None; \
                   for item in items.iter() { \
                   match parse(item) { \
                   Ok(remaining) => return Ok(remaining), \
                   Err(err) if first_err.is_none() => first_err = Some(err), \
                   Err(_) => {} \
                   } \
                   } \
                   match first_err { Some(err) => Err(err), None => Ok(0) } \
                   }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_empty_err_arm_in_fatal_nonfatal_split_issue_5000() {
        // Issue #5000: a structured tuple discriminant splits non-fatal from
        // fatal: `Err((true, err)) => ret_error = err` captures the real
        // error; `Err((false, _err)) => {}` falls through to the next strategy.
        let src = "fn f(r: Result<u8, (bool, E)>) { \
                   let mut ret_error = D; \
                   match r { \
                   Ok(v) => return go(v), \
                   Err((false, _err)) => {} \
                   Err((true, err)) => ret_error = err, \
                   } \
                   }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn flags_lone_empty_err_arm_no_capturing_sibling_issue_5000() {
        // Narrowness guard: a single empty `Err(_) => {}` with no sibling
        // `Err(...)` arm capturing the error is still a blanket swallow and
        // must fire — the partition exemption needs a capturing sibling.
        let src = "fn f(r: Result<u8, E>) { match r { Ok(v) => go(v), Err(_) => {} } }";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    #[test]
    fn flags_empty_err_arm_with_empty_sibling_err_arm_issue_5000() {
        // Narrowness guard: two empty `Err` arms do NOT exempt each other —
        // neither captures the error, so both are swallows. Both must fire.
        let src = "fn f(r: Result<u8, E>) { match r { \
                   Ok(v) => go(v), \
                   Err(a) => {} \
                   Err(b) => {} \
                   } }";
        assert_eq!(run_on(src).len(), 2, "{:?}", run_on(src));
    }

    #[test]
    fn flags_bare_empty_err_arm_beside_unguarded_capturing_sibling_issue_5000() {
        // Narrowness guard: a bare `Err(_) => {}` next to an UNGUARDED capturing
        // sibling (`Err(Specific(e)) => handle(e)`) is NOT a disjoint partition —
        // the wildcard arm swallows every other error class. It must still fire.
        let src = "fn f(r: Result<u8, E>) { match r { \
                   Ok(v) => go(v), \
                   Err(Specific(e)) => handle(e), \
                   Err(_) => {} \
                   } }";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    #[test]
    fn flags_bare_empty_err_arm_beside_guarded_empty_sibling_issue_5000() {
        // Narrowness guard: a GUARDED but EMPTY sibling (`Err(x) if c => {}`)
        // captures nothing, so it does not make the bare `Err(_) => {}` arm a
        // disjoint partition. Both empty arms swallow; the unguarded one fires.
        let src = "fn f(r: Result<u8, E>) { match r { \
                   Ok(v) => go(v), \
                   Err(x) if c(x) => {} \
                   Err(_) => {} \
                   } }";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    #[test]
    fn allows_structured_empty_err_arm_beside_unguarded_capturing_sibling_issue_5000() {
        // Issue #5000: the empty arm carries a STRUCTURED discriminant
        // (`Err(NonFatal(_)) => {}`), so it targets one specific error subset
        // while the unguarded sibling owns the rest — a disjoint partition.
        let src = "fn f(r: Result<u8, E>) { match r { \
                   Ok(v) => go(v), \
                   Err(NonFatal(_)) => {} \
                   Err(other) => handle(other), \
                   } }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_literal_empty_err_arm_beside_unguarded_capturing_sibling_issue_5000() {
        // Issue #5000: a literal discriminant (`Err(NOT_FOUND_CODE) => {}` here
        // a literal `Err(404)`) targets one specific error value, leaving the
        // unguarded sibling to own the rest — a disjoint partition.
        let src = "fn f(r: Result<u8, E>) { match r { \
                   Ok(v) => go(v), \
                   Err(404) => {} \
                   Err(other) => handle(other), \
                   } }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn flags_empty_err_arm_with_trailing_comment_on_previous_arm_issue_3988() {
        // Narrowness guard: a comment trailing the PREVIOUS arm is not a
        // leading justification for the empty `Err(_) => {}` arm, which still
        // silently swallows and must fire.
        let src = "fn f(r: Result<u8, E>) { match r { \
                   Ok(v) => go(v), // trailing comment on the Ok arm\n\
                   Err(_) => {} \
                   } }";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }
}
