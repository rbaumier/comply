//! no-all-duplicated-branches Rust backend.
//!
//! Flag if/else chains that end in an unconditional `else` where every branch
//! has identical code. A chain without a terminal `else` is gated rather than
//! exhaustive, so identical branches are not pointless and are left alone. A
//! chain any of whose conditions reads a `cfg!(...)` predicate is left alone
//! too — see `rust_helpers::expression_reads_cfg_macro` — as is one whose author
//! wrote `#[allow(clippy::if_same_then_else)]`, the clippy lint this mirrors.
//!
//! The `cfg!` carve-out covers the whole chain, deliberately: an all-identical
//! `cfg!` chain is read as a placeholder that documents an axis of variation the
//! code has not needed yet (`if cfg!(target_arch = "aarch64") { "x64" } else
//! { "x64" }` — issue #6810), and that reading is preferred over catching the
//! opposite case, a `cfg!` chain whose arms were meant to differ and drifted
//! into agreement. `no-duplicated-branches` scopes the same carve-out to the
//! compared pair instead, because it judges two adjacent arms rather than the
//! conditional as a whole, so it still reports a duplicate between two ungated
//! arms of a partly gated chain.
//!
//! Match expressions are not currently checked — TODO(#8072).
//! Branches are compared by their ordered AST leaf tokens, so formatting and
//! indentation differences are ignored while string-literal content (including
//! internal whitespace) stays significant.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

/// Collect the text of every leaf (childless) descendant of `node`, in order.
/// Inter-token whitespace is not a node, so formatting differences vanish; a
/// string/char literal's content leaf is preserved verbatim.
fn leaf_tokens<'a>(node: tree_sitter::Node, source: &'a [u8], out: &mut Vec<&'a str>) {
    if node.child_count() == 0 {
        if let Ok(t) = node.utf8_text(source) {
            out.push(t);
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        leaf_tokens(child, source, out);
    }
}

/// Signature of a `block`'s interior — its statements' leaf tokens, excluding
/// the `{`/`}` delimiters (so an empty block yields an empty signature and is
/// not flagged).
fn block_interior_signature(block: tree_sitter::Node, source: &[u8]) -> String {
    let mut out = Vec::new();
    let mut cursor = block.walk();
    for child in block.named_children(&mut cursor) {
        leaf_tokens(child, source, &mut out);
    }
    out.join("\u{1f}")
}

/// Signature of an arbitrary expression/body node, which may be a block
/// expression or a bare expression.
fn node_signature(node: tree_sitter::Node, source: &[u8]) -> String {
    let mut out = Vec::new();
    leaf_tokens(node, source, &mut out);
    out.join("\u{1f}")
}

/// What the rule needs to know about an `if`/`else if` chain: one signature per
/// branch, whether the chain terminates with an unconditional `else { ... }`,
/// and whether any of its conditions reads a `cfg!(...)` predicate.
#[derive(Default)]
struct IfChain {
    branch_signatures: Vec<String>,
    has_terminal_else: bool,
    is_compile_time_gated: bool,
}

/// Walk an `if`/`else if` chain into `chain`.
///
/// The chain only covers every case when a terminal `else` is present;
/// otherwise its body is gated by the conditions, so identical branches are not
/// pointless.
fn collect_if_chain(if_node: tree_sitter::Node, source: &[u8], chain: &mut IfChain) {
    if let Some(condition) = if_node.child_by_field_name("condition")
        && crate::rules::rust_helpers::expression_reads_cfg_macro(condition, source)
    {
        chain.is_compile_time_gated = true;
    }

    if let Some(consequence) = if_node.child_by_field_name("consequence") {
        chain
            .branch_signatures
            .push(block_interior_signature(consequence, source));
    }

    let Some(alternative) = if_node.child_by_field_name("alternative") else {
        return;
    };

    match alternative.kind() {
        "else_clause" => {
            let mut cursor = alternative.walk();
            for child in alternative.children(&mut cursor) {
                if child.kind() == "if_expression" {
                    collect_if_chain(child, source, chain);
                    return;
                }
                if child.kind() == "block" {
                    chain
                        .branch_signatures
                        .push(block_interior_signature(child, source));
                    chain.has_terminal_else = true;
                    return;
                }
            }
        }
        "if_expression" => collect_if_chain(alternative, source, chain),
        "block" => {
            chain
                .branch_signatures
                .push(block_interior_signature(alternative, source));
            chain.has_terminal_else = true;
        }
        _ => {}
    }
}

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["if_expression", "match_expression"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        match node.kind() {
            "if_expression" => {
                if let Some(parent) = node.parent()
                    && parent.kind() == "else_clause"
                {
                    return;
                }

                let mut chain = IfChain::default();
                collect_if_chain(node, source_bytes, &mut chain);
                if chain.is_compile_time_gated {
                    return;
                }

                let branches = &chain.branch_signatures;
                if chain.has_terminal_else
                    && branches.len() >= 2
                    && !branches[0].is_empty()
                    && branches.iter().all(|b| *b == branches[0])
                {
                    // Checked last: only a chain about to be reported can be
                    // suppressed.
                    if crate::rules::rust_helpers::has_clippy_allow(
                        node,
                        source_bytes,
                        "if_same_then_else",
                    ) {
                        return;
                    }

                    let pos = node.start_position();
                    diagnostics.push(Diagnostic {
                            path: std::sync::Arc::clone(&ctx.path_arc),
                            line: pos.row + 1,
                            column: pos.column + 1,
                            rule_id: "no-all-duplicated-branches".into(),
                            message: format!(
                                "All {} branches have identical code \u{2014} the conditional is pointless.",
                                branches.len()
                            ),
                            severity: Severity::Error,
                            span: None,
                        });
                }
            }
            // TODO(#8072): unreachable — tree-sitter nests the arms in a
            // `match_block`, so this scan of the `match_expression`'s own named
            // children never matches one.
            "match_expression" => {
                let mut arm_bodies: Vec<String> = Vec::new();
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    if child.kind() == "match_arm"
                        && let Some(body) = child.child_by_field_name("value")
                    {
                        arm_bodies.push(node_signature(body, source_bytes));
                    }
                }

                if arm_bodies.len() >= 2
                    && !arm_bodies[0].is_empty()
                    && arm_bodies.iter().all(|b| *b == arm_bodies[0])
                {
                    let pos = node.start_position();
                    diagnostics.push(Diagnostic {
                            path: std::sync::Arc::clone(&ctx.path_arc),
                            line: pos.row + 1,
                            column: pos.column + 1,
                            rule_id: "no-all-duplicated-branches".into(),
                            message: format!(
                                "All {} match arms have identical code \u{2014} the match is pointless.",
                                arm_bodies.len()
                            ),
                            severity: Severity::Error,
                            span: None,
                        });
                }
            }
            _ => {}
        }
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
    fn flags_identical_if_else() {
        let source = r#"
fn f() {
    if condition {
        do_something();
    } else {
        do_something();
    }
}
"#;
        let d = run_on(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("2 branches"));
    }

    #[test]
    fn allows_different_branches() {
        let source = r#"
fn f() {
    if condition {
        do_a();
    } else {
        do_b();
    }
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_if_without_else() {
        let source = r#"
fn f() {
    if condition {
        do_something();
    }
}
"#;
        assert!(run_on(source).is_empty());
    }

    // https://github.com/rbaumier/comply/issues/6347
    #[test]
    fn allows_branches_differing_only_in_string_whitespace() {
        let source = r#"
fn f() {
    if self.number.is_some() {
        self.inner.write_str("       ")?;
    } else {
        self.inner.write_str("    ")?;
    }
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_identical_branches_including_strings() {
        let source = r#"
fn f() {
    if c {
        f("x");
    } else {
        f("x");
    }
}
"#;
        let d = run_on(source);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_branches_differing_only_in_formatting() {
        let source = r#"
fn f() {
    if c {
        do_something();
    } else {
        do_something() ;
    }
}
"#;
        let d = run_on(source);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_identical_if_else_if_else_chain() {
        let source = r#"
fn f() {
    if a {
        do_something();
    } else if b {
        do_something();
    } else {
        do_something();
    }
}
"#;
        let d = run_on(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("3 branches"));
    }

    // https://github.com/rbaumier/comply/issues/6800
    #[test]
    fn allows_if_else_if_chain_without_terminal_else() {
        let source = r#"
fn f() {
    while x {
        if let FileTreeItemKind::File(_) = &tree_items[idx].kind {
            should_skip_over -= 1;
            break;
        } else if self.tree.multiple_items_at_path(idx) {
            should_skip_over -= 1;
            break;
        }
    }
}
"#;
        assert!(run_on(source).is_empty());
    }

    // https://github.com/rbaumier/comply/issues/6810
    #[test]
    fn allows_issue_snippet_gated_by_cfg_and_clippy_allow() {
        let source = r#"
#[allow(clippy::if_same_then_else)]
fn arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "x64"
    } else {
        "x64"
    }
}
"#;
        assert!(run_on(source).is_empty());
    }

    /// The `cfg!` gate alone suppresses — the issue snippet's
    /// `#[allow(clippy::if_same_then_else)]` is not what carries it.
    #[test]
    fn allows_identical_branches_under_cfg_macro_condition() {
        let source = r#"
fn arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "x64"
    } else {
        "x64"
    }
}
"#;
        assert!(run_on(source).is_empty());
    }

    /// An author's `#[allow]` of the clippy lint this rule mirrors suppresses on
    /// its own, with no `cfg!` in sight.
    #[test]
    fn allows_identical_branches_under_matching_clippy_allow() {
        let source = r#"
#[allow(clippy::if_same_then_else)]
fn f() {
    if condition {
        do_something();
    } else {
        do_something();
    }
}
"#;
        assert!(run_on(source).is_empty());
    }

    /// An `#[allow]` of some other lint says nothing about this duplication.
    #[test]
    fn flags_identical_branches_under_unrelated_clippy_allow() {
        let source = r#"
#[allow(clippy::needless_return)]
fn f() {
    if condition {
        do_something();
    } else {
        do_something();
    }
}
"#;
        assert_eq!(run_on(source).len(), 1);
    }

    /// Only the conditions gate the chain: a `cfg!` read inside the *bodies*
    /// leaves the branching itself runtime control flow.
    #[test]
    fn flags_identical_branches_reading_cfg_in_their_bodies() {
        let source = r#"
fn f() {
    if flag {
        report(cfg!(unix));
    } else {
        report(cfg!(unix));
    }
}
"#;
        assert_eq!(run_on(source).len(), 1);
    }

    /// A `cfg!` gate on an inner chain does not extend to the outer one, which
    /// is judged on its own conditions.
    #[test]
    fn flags_outer_chain_when_only_a_nested_if_reads_cfg() {
        let source = r#"
fn f() {
    if flag {
        if cfg!(unix) {
            do_something();
        } else {
            do_something();
        }
    } else {
        if cfg!(unix) {
            do_something();
        } else {
            do_something();
        }
    }
}
"#;
        assert_eq!(run_on(source).len(), 1);
    }

    /// The `cfg!` may sit anywhere in the condition expression — the chain is a
    /// compile-time gate either way.
    #[test]
    fn allows_identical_branches_under_compound_cfg_macro_condition() {
        let source = r#"
fn f() {
    if !cfg!(target_os = "windows") && strict {
        do_something();
    } else {
        do_something();
    }
}
"#;
        assert!(run_on(source).is_empty());
    }

    /// A `cfg!` on any arm of the chain gates the whole chain.
    #[test]
    fn allows_identical_branches_when_a_later_else_if_reads_cfg() {
        let source = r#"
fn f() {
    if a {
        do_something();
    } else if cfg!(unix) {
        do_something();
    } else {
        do_something();
    }
}
"#;
        assert!(run_on(source).is_empty());
    }

    /// Negative space: a runtime call named `cfg` is not the `cfg!` macro, so
    /// the chain is ordinary control flow and still flags.
    #[test]
    fn flags_identical_branches_under_runtime_call_named_cfg() {
        let source = r#"
fn f() {
    if cfg() {
        do_something();
    } else {
        do_something();
    }
}
"#;
        let d = run_on(source);
        assert_eq!(d.len(), 1);
    }

    /// Vacuous until #8072 revives the match path; kept as its regression guard.
    #[test]
    fn allows_match_arms_differing_only_in_string_whitespace() {
        let source = r#"
fn f() {
    match k {
        x => w.write_str("    "),
        _ => w.write_str("  "),
    }
}
"#;
        assert!(run_on(source).is_empty());
    }
}
