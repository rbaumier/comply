//! no-commented-out-code — delete dead comments, git history keeps originals.
//!
//! Detection strategy: walk the AST for comment nodes, group consecutive
//! ones (adjacent line comments form one virtual block), strip the comment
//! syntax, then re-parse the resulting text with the SAME tree-sitter
//! grammar the file was parsed with. If the inner parse has no errors and
//! contains at least one "rich" code construct (declaration, call,
//! assignment, control flow), flag the outermost comment of the group.
//!
//! Doc comments (`///`, `//!`, `/**`, `/*!`) are excluded — they
//! legitimately contain example code. Bare prose is excluded by a fast
//! structural filter (`;` or `{` must appear) and by the rich-code check.

mod rust;
mod typescript;

#[cfg(test)]
mod shared_tests;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-commented-out-code",
    description: "Commented-out code is unreviewable, unreachable, and rots.",
    remediation: "Delete the commented-out code. Git history preserves the \
                  original if you need to recover it.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["comments"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}

/// Strip the comment delimiters (`//`, `/*`, `*/`) from a raw comment
/// node's text. Returns `None` for doc comments (`///`, `//!`, `/**`,
/// `/*!`), which are NOT candidates for "commented-out code" because
/// they routinely carry example snippets.
pub(super) fn strip_comment_syntax(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.starts_with("///")
        || raw.starts_with("//!")
        || raw.starts_with("/**")
        || raw.starts_with("/*!")
    {
        return None;
    }
    if let Some(body) = raw.strip_prefix("//") {
        return Some(body.trim_start().to_string());
    }
    if let Some(body) = raw.strip_prefix("/*") {
        let body = body.strip_suffix("*/").unwrap_or(body);
        return Some(body.to_string());
    }
    None
}

/// Fast filter: a block of text is only worth re-parsing if it contains
/// at least one structural marker. Pure prose rarely contains `;` or `{`.
pub(super) fn has_code_shape(text: &str) -> bool {
    text.contains(';') || text.contains('{')
}

/// Collect every node whose kind is in `kinds` into a Vec whose
/// lifetime matches the tree. Can't use `walker::walk_tree` for this
/// because its closure argument has a higher-ranked lifetime and
/// nodes can't escape the closure; the manual cursor walk here
/// preserves the tree lifetime `'t` on each node we push.
///
/// Matches `walk_tree`'s error-skipping semantics: subtrees rooted
/// at ERROR or MISSING nodes are skipped entirely so we don't pull
/// phantom comment nodes out of a parse-error region.
pub(super) fn collect_nodes_of_kinds<'t>(
    tree: &'t tree_sitter::Tree,
    kinds: &[&'static str],
) -> Vec<tree_sitter::Node<'t>> {
    let mut out: Vec<tree_sitter::Node<'t>> = Vec::new();
    let mut cursor = tree.walk();
    'outer: loop {
        let node = cursor.node();
        let bad = node.is_error() || node.is_missing();
        if !bad {
            if kinds.contains(&node.kind()) {
                out.push(node);
            }
            if cursor.goto_first_child() {
                continue;
            }
        }
        loop {
            if cursor.goto_next_sibling() {
                continue 'outer;
            }
            if !cursor.goto_parent() {
                return out;
            }
        }
    }
}

/// Group adjacent comment nodes into virtual blocks. Two comments are
/// considered adjacent if the second one starts on the same line as,
/// or the line immediately after, the first one ends.
pub(super) fn group_adjacent<'tree>(
    comments: &[tree_sitter::Node<'tree>],
) -> Vec<Vec<tree_sitter::Node<'tree>>> {
    let mut groups: Vec<Vec<tree_sitter::Node<'tree>>> = Vec::new();
    for &c in comments {
        let extend = groups
            .last()
            .and_then(|g| g.last())
            .is_some_and(|last| c.start_position().row <= last.end_position().row + 1);
        if extend {
            groups.last_mut().expect("last group exists").push(c);
        } else {
            groups.push(vec![c]);
        }
    }
    groups
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn strip_line_comment() {
        assert_eq!(strip_comment_syntax("// let x = 1;").as_deref(), Some("let x = 1;"));
    }

    #[test]
    fn strip_block_comment() {
        assert_eq!(strip_comment_syntax("/* foo */").as_deref(), Some(" foo "));
    }

    #[test]
    fn reject_triple_slash() {
        assert!(strip_comment_syntax("/// doc").is_none());
    }

    #[test]
    fn reject_double_star() {
        assert!(strip_comment_syntax("/** jsdoc */").is_none());
    }

    #[test]
    fn reject_inner_module_doc() {
        assert!(strip_comment_syntax("//! inner").is_none());
    }

    #[test]
    fn has_code_shape_semicolon() {
        assert!(has_code_shape("foo();"));
    }

    #[test]
    fn has_code_shape_brace() {
        assert!(has_code_shape("if (x) {"));
    }

    #[test]
    fn has_no_code_shape_in_prose() {
        assert!(!has_code_shape("the quick brown fox jumps"));
    }
}
