//! AST backend for react-no-dedup-filter-indexof.
//!
//! Matches `foo.filter(...)` whose callback body or expression contains
//! a `.indexOf(` call — the classic dedup-via-filter idiom.

use crate::diagnostic::{Diagnostic, Severity};

fn body_contains_indexof(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    if node.kind() == "call_expression"
        && let Some(callee) = node.child_by_field_name("function")
            && callee.kind() == "member_expression"
                && let Some(prop) = callee.child_by_field_name("property")
                    && prop.utf8_text(source).ok() == Some("indexOf") {
                        return true;
                    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if body_contains_indexof(child, source) {
            return true;
        }
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let _ = ctx;
    if node.kind() != "call_expression" {
        return;
    }
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    if prop.utf8_text(source).ok() != Some("filter") {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let Some(cb) = args
        .named_children(&mut cursor)
        .find(|c| c.kind() == "arrow_function" || c.kind() == "function_expression")
    else {
        return;
    };
    if !body_contains_indexof(cb, source) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`.filter(... indexOf ...)` is O(n²) dedup — use `[...new Set(arr)]` (O(n)).".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_dedup_filter_indexof() {
        let src = r#"const u = arr.filter((v, i, a) => a.indexOf(v) === i);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_set_dedup() {
        let src = r#"const u = [...new Set(arr)];"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_unrelated_filter() {
        let src = r#"const u = arr.filter(x => x > 0);"#;
        assert!(run(src).is_empty());
    }
}
