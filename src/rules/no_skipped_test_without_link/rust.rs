//! no-skipped-test-without-link backend for Rust.
//!
//! Flags `#[ignore]` attributes without a tracking reference. Rust's
//! `#[ignore]` is the equivalent of TS/JS `.skip` — disables a test
//! until explicitly re-enabled. Require a justification via a preceding
//! doc/line comment with an issue link (`#123`, `ABC-456`, URL).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "attribute_item" {
                return;
            }
            if !is_ignore_attribute(node, source_bytes) {
                return;
            }
            if has_issue_reference_nearby(node, source_bytes) {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-skipped-test-without-link".into(),
                message: "`#[ignore]` without a linked issue — add a comment \
                          above referencing a ticket (`#123`, `ABC-456`, or a \
                          URL) so the skip can be revived later."
                    .into(),
                severity: Severity::Warning,
            });
        });
        diagnostics
    }
}

/// True if the attribute_item wraps `ignore` (with or without a reason arg).
fn is_ignore_attribute(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Ok(text) = node.utf8_text(source) else {
        return false;
    };
    text.contains("ignore")
}

/// Check whether the preceding sibling comment carries an issue reference.
fn has_issue_reference_nearby(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(prev) = node.prev_named_sibling() else {
        return false;
    };
    if prev.kind() != "line_comment" && prev.kind() != "block_comment" {
        return false;
    }
    let Ok(text) = prev.utf8_text(source) else {
        return false;
    };
    text.contains("http://")
        || text.contains("https://")
        || text.bytes().enumerate().any(|(i, b)| {
            b == b'#' && text.as_bytes().get(i + 1).is_some_and(|c| c.is_ascii_digit())
        })
        || has_ticket_key(text)
}

fn has_ticket_key(text: &str) -> bool {
    let bytes = text.as_bytes();
    for i in 0..bytes.len() {
        if !bytes[i].is_ascii_uppercase() {
            continue;
        }
        let mut j = i + 1;
        while j < bytes.len() && bytes[j].is_ascii_uppercase() {
            j += 1;
        }
        if j == i + 1 || j >= bytes.len() || bytes[j] != b'-' {
            continue;
        }
        let mut k = j + 1;
        while k < bytes.len() && bytes[k].is_ascii_digit() {
            k += 1;
        }
        if k > j + 1 {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(
            &CheckCtx {
                path: Path::new("t.rs"),
                source,
            },
            &tree,
        )
    }

    #[test]
    fn flags_ignore_without_comment() {
        let source = "#[ignore]\nfn t() {}";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_ignore_with_issue_ref() {
        let source = "// Tracked in #1234\n#[ignore]\nfn t() {}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_ignore_with_url() {
        let source = "// See https://github.com/o/r/issues/1\n#[ignore]\nfn t() {}";
        assert!(run_on(source).is_empty());
    }
}
