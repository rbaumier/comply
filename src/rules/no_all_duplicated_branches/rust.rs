//! no-all-duplicated-branches Rust backend.
//!
//! Flag if/else chains where every branch has identical code.
//! Also flags match expressions where all arms have identical bodies.
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

/// Signature of an arbitrary expression/body node (used for match-arm values,
/// which may be a block expression or a bare expression).
fn node_signature(node: tree_sitter::Node, source: &[u8]) -> String {
    let mut out = Vec::new();
    leaf_tokens(node, source, &mut out);
    out.join("\u{1f}")
}

fn collect_if_branches(if_node: tree_sitter::Node, source: &[u8]) -> Vec<String> {
    let mut branches = Vec::new();

    if let Some(consequence) = if_node.child_by_field_name("consequence") {
        branches.push(block_interior_signature(consequence, source));
    }

    if let Some(alternative) = if_node.child_by_field_name("alternative") {
        match alternative.kind() {
            "else_clause" => {
                let mut cursor = alternative.walk();
                for child in alternative.children(&mut cursor) {
                    if child.kind() == "if_expression" {
                        let sub = collect_if_branches(child, source);
                        branches.extend(sub);
                        return branches;
                    }
                    if child.kind() == "block" {
                        branches.push(block_interior_signature(child, source));
                    }
                }
            }
            "if_expression" => {
                let sub = collect_if_branches(alternative, source);
                branches.extend(sub);
            }
            "block" => {
                branches.push(block_interior_signature(alternative, source));
            }
            _ => {}
        }
    }

    branches
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

                let branches = collect_if_branches(node, source_bytes);
                if branches.len() >= 2
                    && !branches[0].is_empty()
                    && branches.iter().all(|b| *b == branches[0])
                {
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
