//! elseif-without-else Rust backend — flag `if/else if` chains without
//! a final `else`.
//!
//! Exempt: best-candidate accumulator chains. When every branch body is
//! non-empty and consists exclusively of pure-mutation statements
//! (assignment, compound assignment, or local `let`) with no control-flow
//! exit, function call, or macro, the omitted `else` is intentional —
//! "leave the accumulators unchanged" is the complete remaining behavior,
//! and an empty `else {}` would be noise.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["if_expression"] => |node, source, ctx, diagnostics|
    // Only process top-level if expressions (not those inside else clauses).
    if let Some(parent) = node.parent()
        && parent.kind() == "else_clause" {
            return;
    }

    // Walk the chain to find `else if` and check for final `else`. Track
    // whether every branch body so far diverges (ends in `break`/`return`/
    // `continue` or a never-returning macro): if they all do, the absent
    // `else` is the natural fall-through (e.g. a scanning loop's iteration
    // body), not an overlooked case, so the chain is complete.
    let mut has_else_if = false;
    let mut current: tree_sitter::Node = node;
    let mut last_else_if_node: tree_sitter::Node = node;
    let mut all_branches_diverge = if_consequence_diverges(node, source);
    let mut all_branches_assert = if_consequence_is_pure_assertion(node, source);

    loop {
        let Some(alt) = current.child_by_field_name("alternative") else {
            break;
        };
        if alt.kind() != "else_clause" {
            break;
        }

        // Check if the else_clause contains another if_expression.
        let mut found_nested_if = false;
        let child_count = alt.named_child_count();
        for i in 0..child_count {
            if let Some(child) = alt.named_child(i)
                && child.kind() == "if_expression" {
                    has_else_if = true;
                    last_else_if_node = child;
                    current = child;
                    found_nested_if = true;
                    all_branches_diverge =
                        all_branches_diverge && if_consequence_diverges(child, source);
                    all_branches_assert =
                        all_branches_assert && if_consequence_is_pure_assertion(child, source);
                    break;
            }
        }
        if !found_nested_if {
            // Bare `else { ... }` — chain is complete.
            return;
        }
    }

    if !has_else_if {
        return;
    }

    // Every branch diverges — the missing `else` is the fall-through case.
    if all_branches_diverge {
        return;
    }

    // Every branch body is solely standard assertion-macro calls — a
    // "selective assertion guard". An assertion fires or is statically elided
    // in release, so the absent `else` hides no silent no-op data bug; the
    // chain is intentional.
    if all_branches_assert {
        return;
    }

    // Every branch body is a non-empty run of pure-mutation statements
    // (assignments / local `let`) — a "best-candidate accumulator". Updating
    // running bests and leaving them unchanged for the remaining cases is the
    // complete behavior; an explicit empty `else {}` would be noise.
    if is_pure_mutation_accumulator(node) {
        return;
    }

    let pos = last_else_if_node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elseif-without-else".into(),
        message: "`if/else if` chain without a final `else` \
                  \u{2014} add an `else` block to handle remaining cases."
            .into(),
        severity: Severity::Error,
        span: None,
    });
}

/// True when the `if_expression`'s consequence block ends in a diverging
/// statement.
fn if_consequence_diverges(if_node: tree_sitter::Node, source: &[u8]) -> bool {
    if_node
        .child_by_field_name("consequence")
        .is_some_and(|block| block_diverges(block, source))
}

/// A `block` diverges when its last meaningful child unconditionally exits the
/// enclosing control flow: a `break`/`return`/`continue` expression, or a
/// never-returning std macro. tree-sitter-rust renders `break x;` (trailing
/// `;`) as an `expression_statement` wrapping the `break_expression`, while a
/// tail `break x` (no `;`) is the block's final expression directly — both
/// shapes are handled.
fn block_diverges(block: tree_sitter::Node, source: &[u8]) -> bool {
    let count = block.named_child_count();
    let Some(last) = count.checked_sub(1).and_then(|i| block.named_child(i)) else {
        return false;
    };
    let expr = if last.kind() == "expression_statement" {
        let Some(inner) = last.named_child(0) else {
            return false;
        };
        inner
    } else {
        last
    };
    is_diverging_expr(expr, source)
}

/// True for the expression kinds that never fall through to the next
/// statement.
fn is_diverging_expr(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "break_expression" | "return_expression" | "continue_expression" => true,
        "macro_invocation" => is_never_returning_macro(node, source),
        _ => false,
    }
}

/// The closed, language-defined set of std macros whose return type is `!`:
/// they abort or are statically unreachable, so a branch ending in one handles
/// its case.
fn is_never_returning_macro(node: tree_sitter::Node, source: &[u8]) -> bool {
    node.child_by_field_name("macro")
        .and_then(|name| name.utf8_text(source).ok())
        .is_some_and(|name| matches!(name, "panic" | "unreachable" | "todo" | "unimplemented"))
}

/// The closed, language-defined set of std assertion macros. A chain whose
/// every branch body consists solely of these guards nothing on the missing
/// case: an assertion fires or is statically elided, never a silent no-op.
const ASSERTION_MACROS: [&str; 6] = [
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
];

/// True when the `if_expression`'s consequence is a pure assertion-guard block.
fn if_consequence_is_pure_assertion(if_node: tree_sitter::Node, source: &[u8]) -> bool {
    if_node
        .child_by_field_name("consequence")
        .is_some_and(|block| is_pure_assertion_block(block, source))
}

/// A `block` is a pure assertion guard when it holds at least one statement and
/// every statement is an `expression_statement` wrapping a single assertion
/// `macro_invocation` (comments are ignored). An empty block, or one holding
/// any non-assertion statement, is not — those carry the genuine no-op risk
/// the rule exists to catch.
fn is_pure_assertion_block(block: tree_sitter::Node, source: &[u8]) -> bool {
    let mut assertions = 0usize;
    let count = block.named_child_count();
    for i in 0..count {
        let Some(stmt) = block.named_child(i) else {
            return false;
        };
        if matches!(stmt.kind(), "line_comment" | "block_comment") {
            continue;
        }
        if stmt.kind() != "expression_statement" || stmt.named_child_count() != 1 {
            return false;
        }
        let Some(inner) = stmt.named_child(0) else {
            return false;
        };
        if inner.kind() != "macro_invocation" {
            return false;
        }
        let is_assert = inner
            .child_by_field_name("macro")
            .and_then(|name| name.utf8_text(source).ok())
            .is_some_and(|name| ASSERTION_MACROS.contains(&name));
        if !is_assert {
            return false;
        }
        assertions += 1;
    }
    assertions > 0
}

/// True when the whole `if/else if` chain rooted at `head` is a best-candidate
/// accumulator: every branch body (the `if` consequence plus each `else if`
/// consequence) is a non-empty run of pure-mutation statements. Such a chain
/// intentionally omits the terminal `else` because "leave the running bests
/// unchanged" is the complete behavior for the remaining cases.
fn is_pure_mutation_accumulator(head: tree_sitter::Node) -> bool {
    let mut current = head;
    loop {
        let Some(consequence) = current.child_by_field_name("consequence") else {
            return false;
        };
        if !is_pure_mutation_block(consequence) {
            return false;
        }

        // Advance to the next `else if` in the chain. The caller has already
        // ruled out a terminal bare `else`, so the chain ends at the last
        // `else if`.
        let Some(alt) = current.child_by_field_name("alternative") else {
            return true;
        };
        if alt.kind() != "else_clause" {
            return true;
        }
        let mut next: Option<tree_sitter::Node> = None;
        let count = alt.named_child_count();
        for i in 0..count {
            if let Some(child) = alt.named_child(i)
                && child.kind() == "if_expression"
            {
                next = Some(child);
                break;
            }
        }
        match next {
            Some(if_node) => current = if_node,
            None => return true,
        }
    }
}

/// A `block` is a pure-mutation body when it holds at least one statement and
/// every statement is a pure mutation (comments are ignored). An empty block,
/// or one holding any control-flow exit, function call, or macro, is not —
/// those carry the genuine no-op / missing-case risk the rule exists to catch.
fn is_pure_mutation_block(block: tree_sitter::Node) -> bool {
    let mut mutations = 0usize;
    let count = block.named_child_count();
    for i in 0..count {
        let Some(stmt) = block.named_child(i) else {
            return false;
        };
        if matches!(stmt.kind(), "line_comment" | "block_comment") {
            continue;
        }
        if !is_pure_mutation_stmt(stmt) {
            return false;
        }
        mutations += 1;
    }
    mutations > 0
}

/// A statement is a pure mutation when it is a local `let` binding, or an
/// assignment / compound assignment — directly as a block tail expression or
/// wrapped in an `expression_statement` (the `x = y;` shape). Every other
/// statement kind (function/method calls, macros, `return`/`break`/`continue`,
/// nested control flow) is rejected: only running-state updates qualify.
fn is_pure_mutation_stmt(stmt: tree_sitter::Node) -> bool {
    match stmt.kind() {
        "let_declaration" | "assignment_expression" | "compound_assignment_expr" => true,
        "expression_statement" => stmt.named_child(0).is_some_and(|inner| {
            matches!(
                inner.kind(),
                "assignment_expression" | "compound_assignment_expr"
            )
        }),
        _ => false,
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

    #[test]
    fn flags_else_if_without_else() {
        let src = r#"
fn f(a: bool, b: bool) {
    if a {
        do_a();
    } else if b {
        do_b();
    }
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "elseif-without-else");
    }

    #[test]
    fn allows_else_if_with_else() {
        let src = r#"
fn f(a: bool, b: bool) {
    if a {
        do_a();
    } else if b {
        do_b();
    } else {
        do_c();
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_plain_if_without_else() {
        let src = r#"
fn f(a: bool) {
    if a {
        do_a();
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_guard_exit_loop_with_break() {
        // Classic scanning-loop idiom: both branches `break` out of the loop,
        // the implicit "else" is the loop iteration body.
        let src = r#"
fn f(name_bytes: &[u8]) -> bool {
    let mut i = 0;
    loop {
        if i >= name_bytes.len() {
            break false;
        } else if HEADER_CHARS_H2[name_bytes[i] as usize] == 0 {
            break true;
        }
        i += 1;
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_chain_where_every_branch_returns() {
        let src = r#"
fn f(a: bool, b: bool) -> i32 {
    if a {
        return 1;
    } else if b {
        return 2;
    }
    3
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_chain_where_every_branch_continues() {
        let src = r#"
fn f(items: &[i32]) {
    for x in items {
        if *x < 0 {
            continue;
        } else if *x == 0 {
            continue;
        }
        process(*x);
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_chain_where_every_branch_panics() {
        let src = r#"
fn f(a: bool, b: bool) {
    if a {
        panic!("a");
    } else if b {
        unreachable!();
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_selective_assertion_guard() {
        // Repro from time-rs/time: each branch asserts an invariant for its
        // sub-case; `seconds == 0` legitimately needs no assertion.
        let src = r#"
fn new_ranged_unchecked(seconds: i64) {
    if seconds < 0 {
        debug_assert!(seconds <= 0); // flagged: no final `else`
    } else if seconds > 0 {
        debug_assert!(seconds >= 0);
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_mixed_assertion_macros_with_multiple_statements() {
        let src = r#"
fn f(a: bool, b: bool) {
    if a {
        assert!(a);
        debug_assert_eq!(1, 1);
    } else if b {
        assert_ne!(1, 2);
        debug_assert!(b);
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_branch_with_non_assertion_macro() {
        let src = r#"
fn f(a: bool, b: bool) {
    if a {
        assert!(a);
    } else if b {
        println!("b");
    }
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "elseif-without-else");
    }

    #[test]
    fn flags_branch_with_real_statement() {
        let src = r#"
fn f(a: bool, b: bool) {
    let mut x = 0;
    if a {
        assert!(a);
    } else if b {
        x = 1;
    }
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "elseif-without-else");
    }

    #[test]
    fn flags_empty_branch_in_assertion_chain() {
        // An empty branch is the genuine no-op risk — still flagged.
        let src = r#"
fn f(a: bool, b: bool) {
    if a {
    } else if b {
        assert!(b);
    }
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "elseif-without-else");
    }

    #[test]
    fn allows_assertion_chain_with_final_else() {
        let src = r#"
fn f(a: bool, b: bool) {
    if a {
        assert!(a);
    } else if b {
        debug_assert!(b);
    } else {
        assert!(true);
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_chain_where_one_branch_does_not_diverge() {
        // Negative control: the `else if` body does not diverge, so the
        // missing `else` may be a real omission — still flagged.
        let src = r#"
fn f(a: bool, b: bool) {
    let mut x = 0;
    let mut y = 0;
    if a {
        return;
    } else if b {
        y = 2;
    }
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "elseif-without-else");
    }

    #[test]
    fn allows_best_candidate_accumulator() {
        // Repro from BurntSushi/memchr: "best rare byte" selection — each
        // branch only updates the running-best accumulators, "do nothing" is
        // the correct remaining behavior.
        let src = r#"
fn f(needle: &[u8], ranker: &Ranker) {
    let mut rare1 = needle[0];
    let mut rare2 = needle[1];
    let mut index1 = 0u8;
    let mut index2 = 1u8;
    for (i, &b) in needle.iter().enumerate().take(8).skip(2) {
        if ranker.rank(b) < ranker.rank(rare1) {
            rare2 = rare1;
            index2 = index1;
            rare1 = b;
            index1 = u8::try_from(i).unwrap();
        } else if b != rare1 && ranker.rank(b) < ranker.rank(rare2) {
            rare2 = b;
            index2 = u8::try_from(i).unwrap();
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_accumulator_with_compound_and_let() {
        // Pure-mutation branches mixing compound assignment and a local `let`.
        let src = r#"
fn f(xs: &[i32]) {
    let mut sum = 0;
    let mut max = i32::MIN;
    for &x in xs {
        if x > 0 {
            sum += x;
            let doubled = x * 2;
            max = doubled;
        } else if x < 0 {
            sum -= 1;
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_accumulator_with_a_call_branch() {
        // Negative control: one branch is a function call, not a pure
        // mutation — the missing `else` may be a real omission, still flagged.
        let src = r#"
fn f(a: bool, b: bool) {
    let mut x = 0;
    if a {
        x = 1;
    } else if b {
        record(x);
    }
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "elseif-without-else");
    }

    #[test]
    fn flags_accumulator_with_a_return_branch() {
        // Negative control: a branch returns — not pure mutation, still flagged
        // (and it does not diverge in *every* branch, so the diverge exemption
        // does not apply either).
        let src = r#"
fn f(a: bool, b: bool) -> i32 {
    let mut x = 0;
    if a {
        x = 1;
    } else if b {
        return 2;
    }
    x
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "elseif-without-else");
    }

    #[test]
    fn flags_empty_branch_in_accumulator_chain() {
        // Negative control: an empty branch is the genuine no-op risk — still
        // flagged even when the other branch is a pure mutation.
        let src = r#"
fn f(a: bool, b: bool) {
    let mut x = 0;
    if a {
        x = 1;
    } else if b {
    }
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "elseif-without-else");
    }
}
