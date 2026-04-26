//! no-empty-catch Rust backend — flag silently-ignored errors.
//!
//! Rust has no `catch`, but the equivalent error-swallowing patterns are:
//!
//! - `match r { Ok(_) => ..., Err(_) => {} }` — empty `Err(_)` arm.
//! - `if let Err(_) = r {}` — empty if-let block over an `Err(_)` pattern.
//!
//! A body is considered "empty" when it is a `block` with zero named
//! children AND contains no comment. Comments act as an explicit
//! justification for swallowing the error.

use crate::diagnostic::{Diagnostic, Severity};

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
        path: ctx.path.to_path_buf(),
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
    let Some(pattern) = cond.child_by_field_name("pattern") else { return false };
    pattern_is_err(&pattern, source)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
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
}
