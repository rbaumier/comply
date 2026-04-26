//! dockerfile-label-not-empty tree-sitter backend.
//!
//! Walks each `label_pair` and flags pairs whose value child is missing or
//! whose value text resolves to an empty string after stripping surrounding
//! quotes.

use crate::diagnostic::{Diagnostic, Severity};

fn pair_value_is_empty(pair: tree_sitter::Node, source: &[u8]) -> bool {
    let value = match pair.child_by_field_name("value") {
        Some(v) => v,
        None => return true,
    };
    let text = match std::str::from_utf8(&source[value.byte_range()]) {
        Ok(t) => t,
        Err(_) => return false,
    };
    let trimmed = text.trim();
    matches!(trimmed, "\"\"" | "''" | "")
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "label_pair" { return; }
    if !pair_value_is_empty(node, source) { return; }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "LABEL value must not be empty.".into(),
        severity: Severity::Warning,
        span: Some((node.byte_range().start, node.byte_range().len())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_dockerfile(s, &Check)
    }

    #[test]
    fn flags_empty_double_quoted_value() {
        let src = "FROM alpine\nLABEL maintainer=\"\"\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_empty_single_quoted_value() {
        let src = "FROM alpine\nLABEL maintainer=''\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_non_empty_value() {
        let src = "FROM alpine\nLABEL maintainer=\"alice@example.com\"\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_only_empty_pair_in_multi_pair_label() {
        let src = "FROM alpine\nLABEL good=\"x\" bad=\"\"\n";
        assert_eq!(run(src).len(), 1);
    }
}
