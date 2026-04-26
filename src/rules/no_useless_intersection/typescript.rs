//! no-useless-intersection AST backend — intersection containing `any` or `unknown`.
//!
//! Walks `intersection_type` nodes and flags those whose direct members include
//! a `predefined_type` matching `any` or `unknown`. tree-sitter parses
//! `A & B & C` as a left-recursive `intersection_type(intersection_type(A, B), C)`,
//! so we only need to inspect each `intersection_type` node's own children
//! (any nested intersection is already its own visit).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "intersection_type" {
        return;
    }

    let mut cursor = node.walk();
    let mut found = false;
    for child in node.children(&mut cursor) {
        if child.kind() != "predefined_type" {
            continue;
        }
        let text = std::str::from_utf8(&source[child.byte_range()]).unwrap_or("");
        if text == "any" || text == "unknown" {
            found = true;
            break;
        }
    }

    if !found {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-useless-intersection".into(),
        message: "Intersection with `any` or `unknown` is useless — remove it.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_intersection_with_any() {
        assert_eq!(run_on("type X = Foo & any;").len(), 1);
    }

    #[test]
    fn flags_intersection_with_unknown() {
        assert_eq!(run_on("type X = Foo & unknown;").len(), 1);
    }

    #[test]
    fn flags_any_on_left() {
        assert_eq!(run_on("type X = any & Foo;").len(), 1);
    }

    #[test]
    fn allows_normal_intersection() {
        assert!(run_on("type X = Foo & Bar;").is_empty());
    }

    #[test]
    fn no_false_positive_on_any_prefix() {
        assert!(run_on("type X = anything & Foo;").is_empty());
    }
}
