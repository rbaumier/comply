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

/// True when the receiver is a literal array or `Array.from({ length: N })`
/// that we can prove is *small* (under the threshold). Used to short-circuit
/// the "unknown-receiver returning JSX" branch on toy examples.
fn is_known_small_array_source(recv: tree_sitter::Node<'_>, source: &[u8], min_nodes: usize) -> bool {
    if recv.kind() == "array" {
        let mut cursor = recv.walk();
        let count = recv
            .named_children(&mut cursor)
            .filter(|c| c.kind() != "comment")
            .count();
        return count < min_nodes;
    }
    if recv.kind() == "call_expression" {
        let Some(callee) = recv.child_by_field_name("function") else {
            return false;
        };
        let Ok(callee_text) = callee.utf8_text(source) else {
            return false;
        };
        if callee_text != "Array.from" {
            return false;
        }
        let Some(args) = recv.child_by_field_name("arguments") else {
            return false;
        };
        let mut cursor = args.walk();
        let Some(first) = args.named_children(&mut cursor).next() else {
            return false;
        };
        let Ok(raw) = first.utf8_text(source) else {
            return false;
        };
        if let Some(idx) = raw.find("length") {
            let tail = &raw[idx..];
            if let Some(colon) = tail.find(':') {
                let after = tail[colon + 1..].trim();
                let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(n) = digits.parse::<usize>() {
                    return n < min_nodes;
                }
            }
        }
    }
    false
}

fn large_array_source(recv: tree_sitter::Node<'_>, source: &[u8], min_nodes: usize) -> bool {
    if recv.kind() == "array" {
        let mut cursor = recv.walk();
        let count = recv
            .named_children(&mut cursor)
            .filter(|c| c.kind() != "comment")
            .count();
        return count >= min_nodes;
    }
    if recv.kind() == "call_expression" {
        // `Array.from({ length: N })` pattern.
        let Some(callee) = recv.child_by_field_name("function") else {
            return false;
        };
        let Ok(callee_text) = callee.utf8_text(source) else {
            return false;
        };
        if callee_text != "Array.from" {
            return false;
        }
        let Some(args) = recv.child_by_field_name("arguments") else {
            return false;
        };
        let mut cursor = args.walk();
        let Some(first) = args.named_children(&mut cursor).next() else {
            return false;
        };
        // Look for `length: <number>`.
        let Ok(raw) = first.utf8_text(source) else {
            return false;
        };
        if let Some(idx) = raw.find("length") {
            let tail = &raw[idx..];
            if let Some(colon) = tail.find(':') {
                let after = tail[colon + 1..].trim();
                let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(n) = digits.parse::<usize>() {
                    return n >= min_nodes;
                }
            }
        }
    }
    false
}

fn callback_body_has_content_visibility(cb: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let Ok(text) = cb.utf8_text(source) else {
        return false;
    };
    text.contains("contentVisibility") || text.contains("content-visibility")
}

/// Return true when the callback's body contains JSX (jsx_element or
/// jsx_self_closing_element) — i.e. the `.map(...)` is rendering UI.
fn callback_returns_jsx(cb: tree_sitter::Node<'_>) -> bool {
    fn walk(node: tree_sitter::Node<'_>) -> bool {
        match node.kind() {
            "jsx_element" | "jsx_self_closing_element" | "jsx_fragment" => return true,
            _ => {}
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if walk(child) {
                return true;
            }
        }
        false
    }
    walk(cb)
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let min_nodes = ctx.config.threshold("react-require-content-visibility", "min_nodes", ctx.lang);
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
    let known_large = large_array_source(recv, source, min_nodes);
    // If the receiver is a *known small* literal array or `Array.from({ length: <small> })`,
    // there's no risk worth flagging.
    if !known_large && is_known_small_array_source(recv, source, min_nodes) {
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
    // For unknown receivers (e.g. `items.map(...)`) only flag when the
    // callback actually renders JSX — otherwise we'd false-positive on
    // pure data transforms inside JSX expressions.
    if !known_large && !callback_returns_jsx(cb) {
        return;
    }
    if callback_body_has_content_visibility(cb, source) {
        return;
    }
    let msg = if known_large {
        format!(
            "Large list rendered with `.map()` (>= {min_nodes} items) in JSX without \
             virtualization or `contentVisibility: 'auto'` — paints every off-screen row."
        )
    } else {
        "`.map()` rendering JSX in a JSX expression — wrap with a virtualizer or set \
         `contentVisibility: 'auto'` if the array can be long."
            .to_string()
    };
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        msg,
        Severity::Warning,
    ));
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
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

    #[test]
    fn flags_unknown_receiver_map_returning_jsx() {
        // `items.map(...)` — the receiver length is unknown but the
        // callback renders JSX. Without a virtualizer wrapper, this
        // can paint a thousand off-screen rows.
        let src = r#"const v = <ul>{items.map(i => <li key={i.id}>{i.name}</li>)}</ul>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_unknown_receiver_map_no_jsx() {
        // Pure transform — no rendering, no off-screen paint risk.
        let src = r#"const ids = items.map(i => i.id);"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_unknown_receiver_map_inside_virtualizer() {
        let src = r#"const v = <VirtualList>{items.map(i => <li key={i.id}>{i.name}</li>)}</VirtualList>;"#;
        assert!(run(src).is_empty());
    }
}
