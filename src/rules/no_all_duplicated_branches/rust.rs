//! no-all-duplicated-branches Rust backend.
//!
//! Flag if/else chains where every branch has identical code.
//! Also flags match expressions where all arms have identical bodies.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

fn normalize(text: &str) -> String {
    text.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn block_body_text<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    if node.kind() != "block" {
        return None;
    }
    let start = node.start_byte() + 1;
    let end = node.end_byte().saturating_sub(1);
    if start >= end {
        return Some("");
    }
    std::str::from_utf8(&source[start..end]).ok()
}

fn collect_if_branches(if_node: tree_sitter::Node, source: &[u8]) -> Vec<String> {
    let mut branches = Vec::new();

    if let Some(consequence) = if_node.child_by_field_name("consequence")
        && let Some(text) = block_body_text(consequence, source) {
            branches.push(normalize(text));
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
                    if child.kind() == "block"
                        && let Some(text) = block_body_text(child, source) {
                            branches.push(normalize(text));
                        }
                }
            }
            "if_expression" => {
                let sub = collect_if_branches(alternative, source);
                branches.extend(sub);
            }
            "block" => {
                if let Some(text) = block_body_text(alternative, source) {
                    branches.push(normalize(text));
                }
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
                        && parent.kind() == "else_clause" {
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
                                && let Ok(text) = body.utf8_text(source_bytes) {
                                    arm_bodies.push(normalize(text));
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
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
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
}
