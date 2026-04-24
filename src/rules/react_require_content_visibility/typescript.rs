//! AST backend for react-require-content-visibility.
//!
//! Fires on a `.map(...)` call used inside JSX expression when:
//! - the mapped-over array is a literal with 20+ items, or
//! - the receiver is a length-annotated source (e.g.
//!   `Array.from({ length: N })`) with N >= 20.
//!
//! And the produced JSX does NOT:
//! - use `contentVisibility` in a style literal,
//! - sit inside a JSX element whose tag name contains
//!   `List`/`Virtual`/`Virtuoso`/`Window`.

use crate::diagnostic::{Diagnostic, Severity};

const THRESHOLD: usize = 20;

fn in_jsx_expression(mut node: tree_sitter::Node<'_>) -> bool {
    while let Some(parent) = node.parent() {
        if parent.kind() == "jsx_expression" {
            return true;
        }
        node = parent;
    }
    false
}

fn enclosing_virtualizer_tag(mut node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    while let Some(parent) = node.parent() {
        if parent.kind() == "jsx_element"
            && let Some(open) = parent.child(0)
                && open.kind() == "jsx_opening_element"
                    && let Some(name) = open.child_by_field_name("name")
                        && let Ok(tag) = name.utf8_text(source)
                            && (tag.contains("Virtual")
                                || tag.contains("Virtuoso")
                                || tag.contains("Window")
                                || tag.ends_with("List"))
                            {
                                return true;
                            }
        node = parent;
    }
    false
}

fn large_array_source(recv: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    if recv.kind() == "array" {
        let mut cursor = recv.walk();
        let count = recv
            .named_children(&mut cursor)
            .filter(|c| c.kind() != "comment")
            .count();
        return count >= THRESHOLD;
    }
    if recv.kind() == "call_expression" {
        // `Array.from({ length: N })` pattern.
        let Some(callee) = recv.child_by_field_name("function") else { return false };
        let Ok(callee_text) = callee.utf8_text(source) else { return false };
        if callee_text != "Array.from" {
            return false;
        }
        let Some(args) = recv.child_by_field_name("arguments") else { return false };
        let mut cursor = args.walk();
        let Some(first) = args.named_children(&mut cursor).next() else { return false };
        // Look for `length: <number>`.
        let Ok(raw) = first.utf8_text(source) else { return false };
        if let Some(idx) = raw.find("length") {
            let tail = &raw[idx..];
            if let Some(colon) = tail.find(':') {
                let after = tail[colon + 1..].trim();
                let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(n) = digits.parse::<usize>() {
                    return n >= THRESHOLD;
                }
            }
        }
    }
    false
}

fn callback_body_has_content_visibility(
    cb: tree_sitter::Node<'_>,
    source: &[u8],
) -> bool {
    let Ok(text) = cb.utf8_text(source) else { return false };
    text.contains("contentVisibility") || text.contains("content-visibility")
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
    if prop.utf8_text(source).ok() != Some("map") {
        return;
    }
    if !in_jsx_expression(node) {
        return;
    }
    let Some(recv) = callee.child_by_field_name("object") else { return };
    if !large_array_source(recv, source) {
        return;
    }
    if enclosing_virtualizer_tag(node, source) {
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
    if callback_body_has_content_visibility(cb, source) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "Large list rendered with `.map()` (>= {THRESHOLD} items) in JSX without \
             virtualization or `contentVisibility: 'auto'` — paints every off-screen row."
        ),
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
    fn flags_large_array_literal_map() {
        let src = format!(
            "const v = <ul>{{[{}].map(i => <li key={{i}}>{{i}}</li>)}}</ul>;",
            (0..25).map(|i| i.to_string()).collect::<Vec<_>>().join(",")
        );
        assert_eq!(run(&src).len(), 1);
    }

    #[test]
    fn flags_array_from_large_length() {
        let src = r#"const v = <ul>{Array.from({ length: 100 }).map(i => <li key={i}/>)}</ul>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_small_array_literal() {
        let src = r#"const v = <ul>{[1,2,3].map(i => <li key={i}>{i}</li>)}</ul>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_virtual_list_wrapper() {
        let src = r#"const v = <VirtualList>{Array.from({ length: 100 }).map(i => <li key={i}/>)}</VirtualList>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_content_visibility_style() {
        let src = r#"const v = <ul>{Array.from({ length: 100 }).map(i => <li key={i} style={{ contentVisibility: 'auto' }}/>)}</ul>;"#;
        assert!(run(src).is_empty());
    }
}
