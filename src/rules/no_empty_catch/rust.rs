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
            // A controlled assertion: `Err(Foo) => {}` paired with a
            // sibling arm that diverges (`v => panic!(...)`,
            // `_ => unreachable!()`, `_ => return Err(e)`, …) asserts the
            // result must be this exact error — the empty arm is the
            // success case, not silent error-swallowing.
            if has_diverging_sibling_arm(&node, source) {
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
