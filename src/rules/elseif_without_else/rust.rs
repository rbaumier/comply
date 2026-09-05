//! elseif-without-else Rust backend — flag `if/else if` chains without
//! a final `else`.
//!
//! A chain that compiles without a final `else` is `()`-typed: Rust rejects a
//! value-producing chain left open (E0317), so a missing branch never hands a
//! caller a wrong value. What it can hide is work left undone for the remaining
//! case, so the chain is flagged unless the code already says what happens then.
//!
//! Two properties of the chain as a whole say it:
//!
//! - **pattern dispatch** — an arm tests a refutable pattern (`if let`), so the
//!   chain tries shapes in order instead of carving a value domain into regions.
//!   Falling through means "none of these shapes was present", which is the
//!   whole of the remaining case: there is no region left to name;
//! - **tail expression** — the chain stands directly above the value the
//!   enclosing function answers with, so the chain and its remaining case share
//!   one continuation and no `else` could hold anything the code does not say.
//!
//! Failing those, the chain is complete when every branch answers for itself —
//! and the branches may answer differently, since each way is a claim about one
//! branch and nothing else (see [`branch_answers_for_itself`]):
//!
//! - **no fall-through** — the branch diverges, so control never reaches past it;
//! - **assertion guard** — it only asserts, and an assertion fires or is elided,
//!   never silently does nothing;
//! - **local accumulator** — it only stores a value: a local `let`, an
//!   assignment, or a method call on a `let mut` binding of an enclosing scope,
//!   so "leave it unchanged" is the remaining behavior. The shape proves a write,
//!   not who owns what is written (see [`mutates_local_binding`]).
//!
//! Each is decided from the shape the branch has, not from the kind of node it
//! is spelled with: divergence resolves the callee of a call (the shared
//! [`crate::rules::rust_helpers::node_diverges`]), the accumulator resolves the
//! receiver's binding, and the tail expression is a position in the enclosing
//! block.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{
    block_diverges, block_tail_produces_value, is_comment_node, local_binding_is_mut,
};
use tree_sitter::Node;

crate::ast_check! { on ["if_expression"] => |node, source, ctx, diagnostics|
    // Only process top-level if expressions (not those inside else clauses).
    if let Some(parent) = node.parent()
        && parent.kind() == "else_clause" {
            return;
    }

    let Some(chain) = Chain::of(node) else {
        return;
    };

    if chain.dispatches_on_patterns
        || chain
            .branches
            .iter()
            .all(|body| branch_answers_for_itself(*body, source))
        || remaining_case_is_function_tail(node)
    {
        return;
    }

    let pos = chain.last_else_if.start_position();
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

/// An `if` / `else if` chain the rule has to judge.
struct Chain<'tree> {
    /// The consequence block of the head `if` and of every `else if` after it.
    branches: Vec<Node<'tree>>,
    /// Whether any arm tests a refutable pattern rather than a value — see
    /// [`arm_dispatches_on_a_pattern`].
    dispatches_on_patterns: bool,
    /// The last `else if` of the chain — where the diagnostic points.
    last_else_if: Node<'tree>,
}

impl<'tree> Chain<'tree> {
    /// The chain headed by `head`, or `None` when it is not the rule's subject:
    /// a plain `if` with no `else if`, or a chain a bare `else` already closes.
    fn of(head: Node<'tree>) -> Option<Self> {
        // The subject starts at the first `else if`, so the common shapes — a
        // plain `if` and an `if / else` — are left before collecting anything.
        let Tail::ElseIf(first) = tail_of(head) else {
            return None;
        };
        let mut branches = vec![head.child_by_field_name("consequence")?];
        let mut dispatches_on_patterns = arm_dispatches_on_a_pattern(head);
        let mut last_else_if = first;
        loop {
            branches.push(last_else_if.child_by_field_name("consequence")?);
            dispatches_on_patterns |= arm_dispatches_on_a_pattern(last_else_if);
            match tail_of(last_else_if) {
                Tail::Open => {
                    return Some(Self {
                        branches,
                        dispatches_on_patterns,
                        last_else_if,
                    });
                }
                Tail::BareElse => return None,
                Tail::ElseIf(next) => last_else_if = next,
            }
        }
    }
}

/// True when `if_expression` tests a refutable pattern: its condition is a
/// `let_condition` (`if let <pattern> = <expr>`), on its own or `&&`-joined into
/// a `let_chain`.
///
/// Only the condition's own shape is read. A `let_condition` nested inside a
/// sub-expression binds nothing for the arm, so it is not what this answers
/// about.
fn arm_dispatches_on_a_pattern(if_expression: Node) -> bool {
    let Some(condition) = if_expression.child_by_field_name("condition") else {
        return false;
    };
    match condition.kind() {
        "let_condition" => true,
        "let_chain" => {
            let mut cursor = condition.walk();
            condition
                .named_children(&mut cursor)
                .any(|part| part.kind() == "let_condition")
        }
        _ => false,
    }
}

/// True when `body`, one branch of the chain, settles on its own what the chain
/// leaves for the remaining case — by diverging, by only asserting, or by only
/// storing a value.
///
/// Each way is a claim about this branch alone, so a chain mixing them is as
/// complete as one where every branch answers the same way.
fn branch_answers_for_itself(body: Node, source: &[u8]) -> bool {
    block_diverges(body, source)
        || is_assertion_guard_block(body, source)
        || is_local_accumulator_block(body, source)
}

/// What closes an `if_expression`.
enum Tail<'tree> {
    /// Nothing — the shape the rule flags once an `else if` came before.
    Open,
    /// A bare `else { … }`, which says what happens in the remaining case.
    BareElse,
    /// Another `else if`, which continues the chain.
    ElseIf(Node<'tree>),
}

fn tail_of(if_expression: Node<'_>) -> Tail<'_> {
    let Some(alternative) = if_expression.child_by_field_name("alternative") else {
        return Tail::Open;
    };
    let mut cursor = alternative.walk();
    alternative
        .named_children(&mut cursor)
        .find(|child| child.kind() == "if_expression")
        .map_or(Tail::BareElse, Tail::ElseIf)
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

/// A `block` is an assertion guard when it holds at least one statement and
/// every statement is an `expression_statement` wrapping a single assertion
/// `macro_invocation` (comments are ignored). An empty block, or one holding
/// any non-assertion statement, is not — those carry the genuine no-op risk
/// the rule exists to catch.
fn is_assertion_guard_block(block: Node, source: &[u8]) -> bool {
    let mut assertions = 0usize;
    let count = block.named_child_count();
    for i in 0..count {
        let Some(stmt) = block.named_child(i) else {
            return false;
        };
        if is_comment_node(stmt) {
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

/// A `block` accumulates when it holds at least one statement and every statement
/// only stores a value (comments are ignored) — see [`updates_local_state`]. An
/// empty block, or one holding anything else, is not: those carry the genuine
/// no-op / missing-case risk the rule exists to catch.
fn is_local_accumulator_block(block: Node, source: &[u8]) -> bool {
    let mut updates = 0usize;
    let count = block.named_child_count();
    for i in 0..count {
        let Some(stmt) = block.named_child(i) else {
            return false;
        };
        if is_comment_node(stmt) {
            continue;
        }
        if !updates_local_state(stmt, source) {
            return false;
        }
        updates += 1;
    }
    updates > 0
}

/// A statement stores a value when it is a local `let` binding, an assignment /
/// compound assignment, or a method call mutating a `let mut` binding — either
/// directly as a block tail expression or wrapped in an `expression_statement`
/// (the `x = y;` shape). Every other statement (a free-function call, a macro,
/// `return`/`break`/`continue`, nested control flow) is rejected: it acts rather
/// than stores, so leaving the remaining case unwritten leaves that action
/// unaccounted for.
fn updates_local_state(stmt: Node, source: &[u8]) -> bool {
    match stmt.kind() {
        "let_declaration" | "assignment_expression" | "compound_assignment_expr" => true,
        "call_expression" => mutates_local_binding(stmt, source),
        "expression_statement" => stmt.named_child(0).is_some_and(|inner| match inner.kind() {
            "assignment_expression" | "compound_assignment_expr" => true,
            "call_expression" => mutates_local_binding(inner, source),
            _ => false,
        }),
        _ => false,
    }
}

/// True when `call` is a method call whose receiver is a `let mut` local of the
/// enclosing function — `args.push(x)` under a `let mut args`, a closure capture
/// of one included.
///
/// The receiver's binding is resolved through the scopes around the call, so the
/// test is where the mutated name is declared, not what it is called: a receiver
/// that is a parameter, a field, `self`, or a pattern binding resolves to no
/// local `let mut` and keeps the chain flagged. A local the function declares
/// `mut` and mutates through its own methods stands on the same ground as an
/// assignment to that local — "leave it unchanged" describes the remaining case.
///
/// The claim is the binding's, not the value's: a `let mut` holding a handle to
/// something outside the function (`let mut out = io::stdout()`) qualifies too.
fn mutates_local_binding(call: Node, source: &[u8]) -> bool {
    let Some(callee) = call.child_by_field_name("function") else {
        return false;
    };
    if callee.kind() != "field_expression" {
        return false;
    }
    let Some(receiver) = callee.child_by_field_name("value") else {
        return false;
    };
    if receiver.kind() != "identifier" {
        return false;
    }
    let Ok(name) = receiver.utf8_text(source) else {
        return false;
    };
    local_binding_is_mut(receiver, name, source)
}

/// True when the chain stands directly above the element that produces the
/// enclosing function's value: the chain's block is the body of a `function_item`
/// with a return type other than `()`, and exactly one element separates the
/// chain from the end of that body.
///
/// The chain and its remaining case then share one continuation — the value
/// below — so no `else` block could hold an answer the code does not already
/// give. The claim is that position and nothing more: the branches are not read,
/// so an action-only chain qualifies, and a chain higher up or in a
/// `()`-returning function stays flagged for want of a value to point at, not
/// because its remaining case is any less answered.
fn remaining_case_is_function_tail(head: Node) -> bool {
    let Some(statement) = head.parent() else {
        return false;
    };
    if statement.kind() != "expression_statement" {
        return false;
    }
    let Some(body) = statement.parent() else {
        return false;
    };
    if !body.parent().is_some_and(returns_a_value) {
        return false;
    }
    let mut cursor = body.walk();
    let mut after = body
        .named_children(&mut cursor)
        .skip_while(|child| child.id() != statement.id())
        .skip(1)
        .filter(|child| !is_comment_node(*child));
    let (Some(tail), None) = (after.next(), after.next()) else {
        return false;
    };
    produces_the_functions_value(tail)
}

/// True when `node`, the last element of a function body, produces the value the
/// function answers with: the body's tail expression per the shared
/// [`block_tail_produces_value`], or a `return`, which names the same value one
/// keyword earlier.
fn produces_the_functions_value(node: Node) -> bool {
    block_tail_produces_value(node)
        || node
            .named_child(0)
            .is_some_and(|inner| inner.kind() == "return_expression")
}

/// True when `function` is a `function_item` declaring a return type other than
/// `()`, so its body ends on a value.
fn returns_a_value(function: Node) -> bool {
    function.kind() == "function_item"
        && function
            .child_by_field_name("return_type")
            .is_some_and(|return_type| return_type.kind() != "unit_type")
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
        // A `()`-returning function with a statement below the chain, so the
        // divergence of the branches is the only thing that can exempt it.
        let src = r#"
fn f(a: bool, b: bool) {
    if a {
        return;
    } else if b {
        return;
    }
    cleanup();
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
    fn allows_chain_mixing_an_asserting_and_an_accumulating_branch() {
        // Each branch answers for itself — the assertion fires or is elided, the
        // assignment leaves `x` at its prior value — so the chain is as complete
        // as one whose branches all answer the same way.
        let src = r#"
fn f(a: bool, b: bool) {
    let mut x = 0;
    if a {
        assert!(a);
    } else if b {
        x = 1;
    }
    record(x);
}
"#;
        assert!(run_on(src).is_empty());
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
    fn allows_chain_mixing_a_diverging_and_an_accumulating_branch() {
        // Repro from the issue: libm's `floorf`, vendored verbatim by
        // mooman219/fontdue. One branch stores, the other returns, and the chain
        // sits inside an `else` block, so no whole-chain shape covers it.
        let src = r#"
fn floor(x: f32) -> f32 {
    let mut ui = x.to_bits();
    let e = (((ui >> 23) as i32) & 0xff) - 0x7f;
    if e >= 0 {
        ui &= !(0x007fffff >> e);
    } else {
        if ui >> 31 == 0 {
            ui = 0;
        } else if ui << 1 != 0 {
            return -1.0;
        }
    }
    f32::from_bits(ui)
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_chain_mixing_all_three_self_answering_branch_kinds() {
        // Repro from the issue: an assignment, an assertion and a `return` in one
        // chain. The kinds compose, so enumerating uniform chain shapes cannot
        // close over them.
        let src = r#"
fn m4(a: bool, b: bool, c: bool, mut ui: u32) -> u32 {
    if a {
        ui = 0;
    } else if b {
        debug_assert!(c);
    } else if c {
        return 1;
    }
    ui += 1;
    ui
}
"#;
        assert!(run_on(src).is_empty());
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
    fn flags_accumulator_with_an_acting_branch_inside_a_loop() {
        // Negative control: one branch stores, the other acts on state the
        // function does not declare — nothing answers for the remaining case,
        // and the chain has no function tail below it.
        let src = r#"
fn f(items: &[i32], sink: &mut Vec<i32>) {
    let mut x = 0;
    for &i in items {
        if i > 0 {
            x = i;
        } else if i < 0 {
            sink.push(i);
        }
    }
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "elseif-without-else");
    }

    #[test]
    fn allows_accumulator_mutated_through_a_method_call() {
        // Repro from starship/starship `src/modules/git_status.rs:360-364`:
        // every branch pushes to the same local `Vec`; pushing nothing is the
        // complete remaining behavior.
        let src = r#"
fn get_repo_status(config: &Config, has_untracked: bool) -> Option<RepoStatus> {
    let mut args = vec!["status", "--porcelain=2"];
    if config.ignore_submodules {
        args.push("--ignore-submodules=dirty");
    } else if !has_untracked {
        args.push("--ignore-submodules=untracked");
    }
    let status_output = repo.exec_git(context, &args)?;
    parse(status_output)
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_accumulator_captured_by_a_closure() {
        // Repro from starship/starship `src/modules/git_status.rs:369-375`: the
        // receiver is a `let mut` of the function the closure is written in.
        let src = r##"
fn parse(statuses: Lines) -> RepoStatus {
    let mut repo_status = RepoStatus::default();
    statuses.for_each(|status| {
        if status.starts_with("# branch.ab ") {
            repo_status.set_ahead_behind(status);
        } else if !status.starts_with('#') {
            repo_status.add(status);
        }
    });
    repo_status
}
"##;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_method_call_on_a_parameter() {
        // Negative control: the receiver is a `&mut` parameter, so the mutation
        // reaches state the caller owns — the missing case stays a real omission.
        let src = r#"
fn f(v: &mut Vec<i32>, a: bool, b: bool) {
    if a {
        v.push(1);
    } else if b {
        v.push(2);
    }
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "elseif-without-else");
    }

    #[test]
    fn flags_method_call_on_a_field() {
        // Negative control: the receiver is a field, so the mutation reaches
        // state the enclosing scope does not declare.
        let src = r#"
impl Sink {
    fn f(&mut self, a: bool, b: bool) {
        if a {
            self.buffer.push(1);
        } else if b {
            self.buffer.push(2);
        }
    }
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "elseif-without-else");
    }

    #[test]
    fn flags_method_call_on_a_non_mut_local() {
        // Negative control: a `let` without `mut` cannot be the accumulator —
        // the call acts on something else.
        let src = r#"
fn f(a: bool, b: bool) {
    let sink = Sink::new();
    if a {
        sink.send(1);
    } else if b {
        sink.send(2);
    }
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "elseif-without-else");
    }

    #[test]
    fn allows_chain_where_every_branch_calls_process_exit() {
        // Repro from the issue: `std::process::exit` is declared `-> !`, so the
        // branch cannot fall through.
        let src = r#"
fn write_out(to_file: bool, err: Option<String>) {
    if to_file {
        eprintln!("file path");
        std::process::exit(1);
    } else if let Some(e) = err {
        eprintln!("stdout: {e}");
        std::process::exit(1);
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_never_returning_calls_reached_through_use_declarations() {
        let src = r#"
use std::hint::unreachable_unchecked;
use std::process::{abort, exit};

fn f(a: bool, b: bool, c: bool) {
    if a {
        exit(1);
    } else if b {
        abort();
    } else if c {
        unsafe { unreachable_unchecked() }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_never_returning_call_through_an_imported_module() {
        let src = r#"
use std::process;

fn f(a: bool, b: bool) {
    if a {
        process::exit(1);
    } else if b {
        process::abort();
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_chain_calling_a_never_returning_function_of_the_file() {
        let src = r#"
fn die(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(1)
}

fn f(a: bool, b: bool) {
    if a {
        die("a");
    } else if b {
        die("b");
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_chain_calling_a_same_named_function_that_returns() {
        // Negative control: the name alone proves nothing. This `exit` is
        // declared in the file, returns `()`, and no `use` binds the std one.
        let src = r#"
fn exit(code: i32) {
    println!("would exit with {code}");
}

fn f(a: bool, b: bool) {
    if a {
        exit(1);
    } else if b {
        exit(2);
    }
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "elseif-without-else");
    }

    #[test]
    fn flags_chain_where_only_one_branch_diverges_through_a_call() {
        let src = r#"
fn f(a: bool, b: bool) {
    if a {
        std::process::exit(1);
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
    fn allows_chain_answered_by_the_function_tail_expression() {
        // Repro from the issue: the remaining case is the function's value,
        // written on the line below the chain.
        let src = r#"
fn outcome(ok: bool, s: Option<String>) -> u8 {
    if ok {
        return 1;
    } else if let Some(v) = s {
        if v.starts_with("error") {
            return 2;
        }
    }
    0
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_action_chain_above_the_function_tail_expression() {
        // Repro from BurntSushi/ripgrep `crates/core/flags/defs.rs:3879`: a
        // plain boolean chain whose branches act, standing above the `Ok(())`
        // the function answers with. The criterion covers a chain that reads no
        // branch content, and this is the shape that costs.
        let src = r#"
fn update(&self, v: FlagValue, args: &mut LowArgs) -> anyhow::Result<()> {
    if v.unwrap_switch() {
        args.mode.update(Mode::Search(SearchMode::JSON));
    } else if matches!(args.mode, Mode::Search(SearchMode::JSON)) {
        // --no-json only reverts to the default mode if the mode is JSON,
        // otherwise it's a no-op.
        args.mode.update(Mode::Search(SearchMode::Standard));
    }
    Ok(())
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_chain_with_no_tail_expression_below_it() {
        // Negative control: same chain, nothing below it — the remaining case is
        // written nowhere. The function returns `()` of necessity: a function
        // that returns a value has to end on one.
        let src = r#"
fn outcome(ok: bool, second: bool) {
    if ok {
        record(1);
    } else if second {
        record(2);
    }
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "elseif-without-else");
    }

    #[test]
    fn flags_chain_separated_from_the_tail_expression() {
        // Negative control: a statement stands between the chain and the tail,
        // so the tail answers for that statement too, not for the chain.
        let src = r#"
fn outcome(ok: bool, b: bool) -> u8 {
    if ok {
        record(1);
    } else if b {
        record(2);
    }
    record(3);
    0
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "elseif-without-else");
    }

    #[test]
    fn flags_chain_whose_function_returns_unit() {
        // Negative control: the statement below the chain is not a value for
        // the remaining case, it is the next thing the function does.
        let src = r#"
fn f(a: bool, b: bool) {
    if a {
        record(1);
    } else if b {
        record(2);
    }
    finish()
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "elseif-without-else");
    }

    #[test]
    fn allows_chain_whose_branches_call_a_renamed_never_returning_import() {
        // The name is the author's; the `use` is what says what it calls.
        let src = r#"
use std::process::exit as die;

fn f(a: bool, b: bool) {
    if a {
        die(1);
    } else if b {
        die(2);
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_chain_answered_by_a_block_like_tail_expression() {
        // The tail is a `match`, which the grammar wraps in an
        // `expression_statement` like any statement — without the `;` that would
        // make it one.
        let src = r#"
fn outcome(a: bool, b: bool) -> u8 {
    if a {
        return 1;
    } else if b {
        record(2);
    }
    match fallback() {
        Kind::A => 1,
        _ => 0,
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_chain_answered_by_a_final_return() {
        // `return 0;` names the value for the remaining case as plainly as a
        // bare `0` does.
        let src = r#"
fn outcome(a: bool, b: bool) -> u8 {
    if a {
        record(1);
    } else if b {
        record(2);
    }
    return 0;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_chain_above_a_discarded_statement() {
        // Negative control for the `;`: the statement below the chain is the
        // next thing the function does, not the value it answers with. What is
        // below the chain is never read for divergence either — only the
        // branches are.
        let src = r#"
fn outcome(a: bool, b: bool) -> u8 {
    if a {
        record(1);
    } else if b {
        record(2);
    }
    panic!("unsupported");
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "elseif-without-else");
    }

    #[test]
    fn allows_if_let_pattern_chain() {
        // Repro from harfbuzz/ttf-parser `src/tables/glyf.rs:128`: each arm tries
        // a shape of the accumulated contour. Falling through means no on-curve
        // point was present, which is the whole of the remaining case.
        let src = r#"
impl Outline {
    fn close(&mut self) {
        if let (Some(p), Some(off)) = (self.first_on_curve, self.last_off_curve) {
            self.quad_to(off.0, off.1, p.0, p.1);
        } else if let Some(p) = self.first_on_curve {
            self.line_to(p.0, p.1);
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_chain_mixing_a_boolean_and_a_pattern_arm() {
        // One refutable pattern is enough: the chain no longer carves a value
        // domain into regions, so there is no region left for an `else` to name.
        let src = r#"
fn f(a: bool, s: Option<u8>) {
    if a {
        record(1);
    } else if let Some(v) = s {
        record(v);
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_chain_whose_pattern_arm_is_a_let_chain() {
        // `let` joined by `&&` binds for the arm just as a bare `if let` does.
        let src = r#"
fn f(a: bool, s: Option<u8>) {
    if a {
        record(1);
    } else if a && let Some(v) = s {
        record(v);
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_chain_whose_conditions_are_all_boolean_tests() {
        // Negative control for the pattern exemption: a value domain carved into
        // regions really can leave one out.
        let src = r#"
fn f(a: i32) {
    if a > 0 {
        record(1);
    } else if a < 0 {
        record(2);
    }
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
