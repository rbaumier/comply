//! no-duplicated-branches Rust backend.
//!
//! Flag branches with identical bodies in `if / else if / else` chains
//! (outermost `if_expression` only).
//!
//! ## `if let` chains (pattern-binding mode)
//!
//! When a chain has a branch whose condition introduces a `let` binding — a
//! bare `if let PAT = EXPR` (`let_condition`) or a `&&` let-chain (`let_chain`,
//! `if let A && let B`) — the rule switches to comparing `(condition_text,
//! body_text)` instead of
//! body text alone. Two `if let` branches can share an identical body that
//! references a pattern-bound name (`r`, `n`, …) while the `r` in each
//! branch is a distinct binding introduced by a different pattern — a
//! syntactic match that is not a semantic duplicate. Only when both the
//! condition and the body are textually identical does the duplicate flag
//! still fire, which is the genuine case (two literally-equal `if let`
//! branches).
//!
//! ## Compile-time gates
//!
//! An arm whose condition reads a `cfg!(...)` predicate is left alone: the
//! compiler keeps it for some targets and drops it for others, so a body it
//! shares with its neighbour is a per-configuration variant rather than a
//! duplicate to merge away. A duplicate between two ungated arms of the same
//! chain still flags.
//!
//! ## Dedup
//!
//! A single duplicate line is reported at most once per chain.

use rustc_hash::FxHashSet;
use crate::diagnostic::{Diagnostic, Severity};

struct Branch {
    line: usize,
    condition: String,
    body: String,
    is_let_condition: bool,
    condition_reads_cfg: bool,
}

crate::ast_check! { on ["if_expression"] => |node, source, ctx, diagnostics|
    check_if_branches(node, source, ctx, diagnostics);
}

fn check_if_branches(
    node: tree_sitter::Node,
    source: &[u8],
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Only process the outermost if in an else-if chain.
    if let Some(parent) = node.parent()
        && parent.kind() == "else_clause"
    {
        return;
    }

    // Defer to an author's `#[allow(clippy::if_same_then_else)]` on the
    // if-expression: this rule mirrors that clippy lint, so the explicit
    // machine-readable suppression of it overrides the duplicate-branch flag.
    if crate::rules::rust_helpers::has_clippy_allow(node, source, "if_same_then_else") {
        return;
    }

    let mut branches: Vec<Branch> = Vec::new();
    collect_if_branches(node, source, &mut branches);

    if branches.len() < 2 {
        return;
    }

    let pattern_binding_mode = chain_has_let_condition(&branches);

    let key = |b: &Branch| -> String {
        if pattern_binding_mode {
            format!("{}\u{1}{}", b.condition, b.body)
        } else {
            b.body.clone()
        }
    };

    // Only directly-adjacent arms are trivially mergeable (`A || B`).
    // Non-adjacent arms with an identical body are separated by a distinct
    // arm; merging them would require reordering the chain, which changes
    // top-to-bottom evaluation when conditions overlap. Compare each arm
    // against its immediate predecessor only.
    // An arm drops out of the comparison when it has no body to compare, or
    // when its condition reads `cfg!` — see "Compile-time gates" above.
    let is_excluded = |b: &Branch| b.body.is_empty() || b.condition_reads_cfg;

    let mut reported: FxHashSet<usize> = FxHashSet::default();
    for j in 1..branches.len() {
        if is_excluded(&branches[j]) || is_excluded(&branches[j - 1]) {
            continue;
        }
        if key(&branches[j]) == key(&branches[j - 1]) && reported.insert(branches[j].line) {
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: branches[j].line,
                column: 1,
                rule_id: "no-duplicated-branches".into(),
                message: "This branch has the same body as the previous branch \u{2014} merge conditions or remove the duplicate.".into(),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}

fn chain_has_let_condition(branches: &[Branch]) -> bool {
    branches.iter().any(|b| b.is_let_condition)
}

fn collect_if_branches(node: tree_sitter::Node, source: &[u8], branches: &mut Vec<Branch>) {
    let cond_node = node.child_by_field_name("condition");
    let condition = cond_node
        .and_then(|c| c.utf8_text(source).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    // Pattern-binding mode engages when the condition introduces a `let`
    // binding the branch body can reference: a bare `if let PAT = EXPR`
    // (`let_condition`) or a `&&` let-chain (`let_chain`, `if let A && let B`).
    // Both are the condition field's own node kind, so a `let` nested deeper in
    // the condition (e.g. inside a closure) — which binds nothing visible to the
    // body — correctly does not qualify.
    let is_let_condition =
        cond_node.is_some_and(|c| matches!(c.kind(), "let_condition" | "let_chain"));
    let condition_reads_cfg =
        cond_node.is_some_and(|c| crate::rules::rust_helpers::expression_reads_cfg_macro(c, source));

    if let Some(body) = node.child_by_field_name("consequence") {
        let line = body.start_position().row + 1;
        let text = body_text(&body, source);
        branches.push(Branch {
            line,
            condition,
            body: text,
            is_let_condition,
            condition_reads_cfg,
        });
    }

    if let Some(alt) = node.child_by_field_name("alternative") {
        let mut cursor = alt.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                match child.kind() {
                    "if_expression" => {
                        collect_if_branches(child, source, branches);
                        return;
                    }
                    "block" => {
                        let line = child.start_position().row + 1;
                        let text = body_text(&child, source);
                        branches.push(Branch {
                            line,
                            condition: String::new(),
                            body: text,
                            is_let_condition: false,
                            condition_reads_cfg: false,
                        });
                        return;
                    }
                    _ => {}
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }
}

fn body_text(node: &tree_sitter::Node, source: &[u8]) -> String {
    let mut parts = Vec::new();
    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i)
            && let Ok(t) = child.utf8_text(source)
        {
            parts.push(t.trim().to_string());
        }
    }
    parts.join("\n")
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
    fn flags_duplicate_if_else() {
        let src = r#"fn f() {
    if a {
        do_something();
    } else {
        do_something();
    }
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_different_branches() {
        let src = r#"fn f() {
    if a {
        foo();
    } else {
        bar();
    }
}"#;
        assert!(run_on(src).is_empty());
    }

    // https://github.com/rbaumier/comply/issues/6810
    #[test]
    fn allows_duplicate_branches_under_cfg_macro_condition() {
        let src = r#"fn arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "x64"
    } else {
        "x64"
    }
}"#;
        assert!(run_on(src).is_empty());
    }

    /// The gate covers the compared pair only: two ungated arms sharing a body
    /// are a real duplicate on every target, `cfg!` elsewhere in the chain or not.
    #[test]
    fn flags_adjacent_runtime_duplicate_in_a_chain_with_a_cfg_arm() {
        let src = r#"fn f() {
    if cfg!(unix) {
        foo();
    } else if b {
        bar();
    } else if c {
        baz();
    } else {
        baz();
    }
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    /// The gated arm may sit anywhere in the chain, not just first.
    #[test]
    fn allows_duplicate_branches_when_a_later_else_if_reads_cfg() {
        let src = r#"fn f() {
    if a {
        foo();
    } else if cfg!(unix) {
        bar();
    } else {
        bar();
    }
}"#;
        assert!(run_on(src).is_empty());
    }

    /// `cfg()` is a call expression, not the `cfg!` macro — nothing gates the
    /// chain, so the adjacent duplicate bodies still flag.
    #[test]
    fn flags_duplicate_branches_under_runtime_call_named_cfg() {
        let src = r#"fn f() {
    if cfg() {
        do_something();
    } else {
        do_something();
    }
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_single_branch() {
        let src = r#"fn f() {
    if a {
        foo();
    }
}"#;
        assert!(run_on(src).is_empty());
    }

    /// FP observed on src/rules/no_redundant_assignment/typescript.rs:30-35:
    /// three `if let` branches with the same `r.trim_start()` body. The `r`
    /// in each branch is a distinct pattern binding.
    #[test]
    fn allows_if_let_chain_with_distinct_patterns() {
        let src = r#"fn f(trimmed: &str) -> &str {
    let rest = if let Some(r) = trimmed.strip_prefix("let ") {
        r.trim_start()
    } else if let Some(r) = trimmed.strip_prefix("const ") {
        r.trim_start()
    } else if let Some(r) = trimmed.strip_prefix("var ") {
        r.trim_start()
    } else {
        trimmed
    };
    rest
}"#;
        assert!(run_on(src).is_empty());
    }

    /// Two `if let` branches with identical patterns AND identical bodies
    /// ARE a real duplicate — the same match, the same action.
    #[test]
    fn flags_two_identical_if_let_branches() {
        let src = r#"fn f(trimmed: &str) -> Option<&str> {
    if let Some(r) = trimmed.strip_prefix("let ") {
        Some(r.trim_start())
    } else if let Some(r) = trimmed.strip_prefix("let ") {
        Some(r.trim_start())
    } else {
        None
    }
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    /// #7328 (biomejs/biome use_inline_script_id.rs): two `&&` let-chain
    /// branches whose identical-text bodies each reference a `name` bound by
    /// that branch's OWN let-chain (from different sources) are not duplicates.
    /// The condition parses as a `let_chain`, so pattern-binding mode must
    /// engage for it just as it does for a bare `let_condition`.
    #[test]
    fn allows_let_chain_branches_with_distinct_patterns() {
        let src = r#"fn f(m: M, set: &mut S) {
    if let Some(pm) = m.as_prop() && let Some(name) = pm.name() {
        set.insert(name);
    } else if let Some(sh) = m.as_shorthand() && let Some(name) = sh.name() {
        set.insert(name);
    }
}"#;
        assert!(run_on(src).is_empty());
    }

    /// A `&&` let-chain repeated verbatim (identical condition AND body) is a
    /// genuine duplicate and still fires under pattern-binding mode.
    #[test]
    fn flags_two_identical_let_chain_branches() {
        let src = r#"fn f(m: M, set: &mut S) {
    if let Some(pm) = m.as_prop() && let Some(name) = pm.name() {
        set.insert(name);
    } else if let Some(pm) = m.as_prop() && let Some(name) = pm.name() {
        set.insert(name);
    }
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    /// Only the top-level condition node kind flips a branch into pattern-binding
    /// mode: a `let_condition` inside a closure embedded in the condition binds
    /// nothing the branch body can see (the condition node is a `call_expression`
    /// here), so two such non-let branches with identical bodies stay a
    /// trivially-mergeable duplicate and are still flagged.
    #[test]
    fn flags_when_let_condition_is_inside_a_closure() {
        let src = r#"fn f(a: Vec<i32>, b: Vec<i32>) {
    if a.iter().any(|x| if let Some(y) = x.first() { *y > 0 } else { false }) {
        foo();
    } else if b.iter().any(|x| if let Some(y) = x.first() { *y > 0 } else { false }) {
        foo();
    }
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    /// Three branches with identical bodies should report TWO diagnostics
    /// (one per duplicate line), not three — the pairwise loop used to
    /// emit line `j` once per earlier match.
    #[test]
    fn dedups_three_identical_branches() {
        let src = r#"fn f(a: bool, b: bool) {
    if a {
        foo();
    } else if b {
        foo();
    } else {
        foo();
    }
}"#;
        // Three branches with the same body: lines for branches 2 and 3
        // are duplicates (line of branch 1 is the "reference"), so 2
        // diagnostics — not 3 (the old pairwise loop emitted 3).
        assert_eq!(run_on(src).len(), 2);
    }

    /// #1493: two arms in a flat chain produce the same value for distinct,
    /// non-adjacent reasons (bat's `RangeCheckResult`). The duplicate arms
    /// are separated by `BeforeOrBetweenRanges`, so merging them would
    /// require reordering the chain — not trivially mergeable, not flagged.
    #[test]
    fn allows_non_adjacent_duplicate_in_chain() {
        let src = r#"fn check(line: usize) -> RangeCheckResult {
    if ranges.iter().any(|r| r.is_inside(line)) {
        RangeCheckResult::InRange
    } else if matches!(max_buffered, Final(n) if line > n) {
        RangeCheckResult::AfterLastRange
    } else if line < self.upper {
        RangeCheckResult::BeforeOrBetweenRanges
    } else {
        RangeCheckResult::AfterLastRange
    }
}"#;
        assert!(run_on(src).is_empty());
    }

    /// #1493: the same value reached through different nesting levels (bat's
    /// `PagingMode::Never`) is captured as part of the consequence body text,
    /// never enumerated as a sibling branch — so it is not flagged.
    #[test]
    fn allows_cross_nesting_duplicate_value() {
        let src = r#"fn paging(&self) -> PagingMode {
    if reading_from_stdin && !list_themes {
        if self.interactive_output && !stdin_is_terminal() {
            PagingMode::QuitIfOneScreen
        } else {
            PagingMode::Never
        }
    } else if self.interactive_output {
        PagingMode::QuitIfOneScreen
    } else {
        PagingMode::Never
    }
}"#;
        assert!(run_on(src).is_empty());
    }

    /// Negative-space guard: a directly-adjacent identical arm in a longer
    /// chain is still trivially mergeable (`A || B`) and stays flagged.
    #[test]
    fn flags_adjacent_duplicate_in_longer_chain() {
        let src = r#"fn f(a: bool, b: bool, c: bool) {
    if a {
        foo();
    } else if b {
        bar();
    } else if c {
        baz();
    } else {
        baz();
    }
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    /// #6653 (sharkdp/pastel src/distinct.rs:149): an `#[allow(clippy::if_same_then_else)]`
    /// on the if-expression is the author's explicit opt-out; the duplicate
    /// adjacent branches must not be flagged.
    #[test]
    fn allows_if_chain_with_clippy_allow() {
        let src = r#"fn f(result: Pair, params: Params, rng: Rng) -> usize {
    #[allow(clippy::if_same_then_else)]
    if result.closest_pair.0 < params.num_fixed_colors {
        result.closest_pair.1
    } else if result.closest_pair.1 < params.num_fixed_colors {
        result.closest_pair.0
    } else if rng.random() {
        result.closest_pair.0
    } else {
        result.closest_pair.1
    }
}"#;
        assert!(run_on(src).is_empty());
    }

    /// Same chain WITHOUT the allow: the adjacent identical branches are still
    /// a real duplicate and stay flagged (the rule is not gutted).
    #[test]
    fn flags_if_chain_without_clippy_allow() {
        let src = r#"fn f(result: Pair, params: Params, rng: Rng) -> usize {
    if result.closest_pair.0 < params.num_fixed_colors {
        result.closest_pair.1
    } else if result.closest_pair.1 < params.num_fixed_colors {
        result.closest_pair.0
    } else if rng.random() {
        result.closest_pair.0
    } else {
        result.closest_pair.1
    }
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    /// An UNRELATED `#[allow]` does not suppress the duplicate-branch flag.
    #[test]
    fn flags_if_chain_with_unrelated_clippy_allow() {
        let src = r#"fn f(result: Pair, params: Params, rng: Rng) -> usize {
    #[allow(clippy::needless_return)]
    if result.closest_pair.0 < params.num_fixed_colors {
        result.closest_pair.1
    } else if result.closest_pair.1 < params.num_fixed_colors {
        result.closest_pair.0
    } else if rng.random() {
        result.closest_pair.0
    } else {
        result.closest_pair.1
    }
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn does_not_check_match_arms() {
        let src = r#"fn f(x: u8) -> u8 {
    match x {
        0 => foo(),
        1 => foo(),
        2 => foo(),
        _ => 0,
    }
}"#;
        assert!(run_on(src).is_empty());
    }
}
