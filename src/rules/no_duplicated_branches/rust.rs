//! no-duplicated-branches Rust backend.
//!
//! Flag branches with identical bodies in `if / else if / else` chains
//! (outermost `if_expression` only).
//!
//! ## `if let` chains (pattern-binding mode)
//!
//! When a chain contains at least one `let_condition` (`if let PAT = EXPR`),
//! the rule switches to comparing `(condition_text, body_text)` instead of
//! body text alone. Two `if let` branches can share an identical body that
//! references a pattern-bound name (`r`, `n`, …) while the `r` in each
//! branch is a distinct binding introduced by a different pattern — a
//! syntactic match that is not a semantic duplicate. Only when both the
//! condition and the body are textually identical does the duplicate flag
//! still fire, which is the genuine case (two literally-equal `if let`
//! branches).
//!
//! ## Dedup
//!
//! A single duplicate line is reported at most once per chain.

use crate::diagnostic::{Diagnostic, Severity};

struct Branch {
    line: usize,
    condition: String,
    body: String,
    is_let_condition: bool,
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

    let mut reported: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for j in 1..branches.len() {
        if branches[j].body.is_empty() {
            continue;
        }
        let kj = key(&branches[j]);
        for i in 0..j {
            if branches[i].body.is_empty() {
                continue;
            }
            if key(&branches[i]) == kj && reported.insert(branches[j].line) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: branches[j].line,
                    column: 1,
                    rule_id: "no-duplicated-branches".into(),
                    message: "This branch has the same body as another branch \u{2014} merge conditions or remove the duplicate.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
                break;
            }
        }
    }
}

fn chain_has_let_condition(branches: &[Branch]) -> bool {
    branches.iter().any(|b| b.is_let_condition)
}

fn collect_if_branches(
    node: tree_sitter::Node,
    source: &[u8],
    branches: &mut Vec<Branch>,
) {
    let cond_node = node.child_by_field_name("condition");
    let condition = cond_node
        .and_then(|c| c.utf8_text(source).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let is_let_condition = cond_node.is_some_and(|c| c.kind() == "let_condition");

    if let Some(body) = node.child_by_field_name("consequence") {
        let line = body.start_position().row + 1;
        let text = body_text(&body, source);
        branches.push(Branch {
            line,
            condition,
            body: text,
            is_let_condition,
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
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
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
