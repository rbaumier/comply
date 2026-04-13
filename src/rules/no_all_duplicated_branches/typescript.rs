//! no-all-duplicated-branches backend — flag if/else chains where every
//! branch has identical code.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

#[derive(Debug)]
pub struct Check;

/// Normalize a block's source text for comparison: collapse whitespace.
fn normalize(text: &str) -> String {
    text.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Extract the body text of a statement_block node (the `{ ... }` block).
/// Returns the text between the braces.
fn block_body_text<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    if node.kind() != "statement_block" {
        return None;
    }
    // Get inner text: skip leading `{` and trailing `}`.
    let start = node.start_byte() + 1;
    let end = node.end_byte().saturating_sub(1);
    if start >= end {
        return Some("");
    }
    std::str::from_utf8(&source[start..end]).ok()
}

/// Collect all branch bodies from an if_statement (including else-if chains).
fn collect_branches(
    if_node: tree_sitter::Node,
    source: &[u8],
) -> Vec<String> {
    let mut branches = Vec::new();

    // Get the consequence (then block).
    if let Some(consequence) = if_node.child_by_field_name("consequence")
        && let Some(text) = block_body_text(consequence, source) {
            branches.push(normalize(text));
        }

    // Get the alternative (else / else-if).
    if let Some(alternative) = if_node.child_by_field_name("alternative") {
        match alternative.kind() {
            "else_clause" => {
                // Could contain another if_statement (else if) or a statement_block (else).
                let mut cursor = alternative.walk();
                for child in alternative.children(&mut cursor) {
                    if child.kind() == "if_statement" {
                        // Recurse into else-if.
                        let sub = collect_branches(child, source);
                        branches.extend(sub);
                        return branches;
                    }
                    if child.kind() == "statement_block"
                        && let Some(text) = block_body_text(child, source) {
                            branches.push(normalize(text));
                        }
                }
            }
            "if_statement" => {
                let sub = collect_branches(alternative, source);
                branches.extend(sub);
            }
            "statement_block" => {
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
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();

        walk_tree(tree, |node| {
            if node.kind() != "if_statement" {
                return;
            }

            // Only flag top-level if (not else-if chains — they'll be caught
            // from the parent if_statement).
            if let Some(parent) = node.parent()
                && parent.kind() == "else_clause" {
                    return;
                }

            let branches = collect_branches(node, source_bytes);

            // Need at least 2 branches (if + else) and non-empty bodies.
            if branches.len() >= 2
                && !branches[0].is_empty()
                && branches.iter().all(|b| *b == branches[0])
            {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "no-all-duplicated-branches".into(),
                    message: format!(
                        "All {} branches have identical code — the conditional is pointless.",
                        branches.len()
                    ),
                    severity: Severity::Error,
                    span: None,
                });
            }
        });

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_identical_if_else() {
        let source = r#"
if (condition) {
    doSomething();
} else {
    doSomething();
}
"#;
        let d = run_on(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("2 branches"));
    }

    #[test]
    fn flags_identical_if_else_if_else() {
        let source = r#"
if (a) {
    doSomething();
} else if (b) {
    doSomething();
} else {
    doSomething();
}
"#;
        let d = run_on(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("3 branches"));
    }

    #[test]
    fn allows_different_branches() {
        let source = r#"
if (condition) {
    doA();
} else {
    doB();
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_if_without_else() {
        let source = r#"
if (condition) {
    doSomething();
}
"#;
        assert!(run_on(source).is_empty());
    }
}
