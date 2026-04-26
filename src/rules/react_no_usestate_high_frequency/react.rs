//! AST backend for react-no-usestate-high-frequency.
//!
//! Shapes detected:
//! - `addEventListener("mousemove"|"scroll"|..., handler)` where
//!   `handler` is an inline arrow/function body that calls `setX`.
//! - JSX attributes `onMouseMove`/`onScroll`/`onPointerMove`/... where
//!   the attribute value is an inline arrow calling `setX`.

use crate::diagnostic::{Diagnostic, Severity};

const HIGH_FREQ_EVENTS: &[&str] = &["mousemove", "scroll", "resize", "pointermove", "wheel"];
const HIGH_FREQ_JSX_PROPS: &[&str] = &[
    "onMouseMove",
    "onScroll",
    "onPointerMove",
    "onWheel",
    "onDrag",
    "onDragOver",
    "onTouchMove",
];

fn callback_contains_setstate(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    if node.kind() == "call_expression"
        && let Some(callee) = node.child_by_field_name("function")
            && callee.kind() == "identifier"
                && let Ok(text) = callee.utf8_text(source)
                    && text.starts_with("set")
                        && text.len() > 3
                        && text.as_bytes()[3].is_ascii_uppercase()
                    {
                        return true;
                    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if callback_contains_setstate(child, source) {
            return true;
        }
    }
    false
}

fn check_addeventlistener<'a>(
    node: tree_sitter::Node<'a>,
    source: &'a [u8],
) -> Option<tree_sitter::Node<'a>> {
    if node.kind() != "call_expression" {
        return None;
    }
    let callee = node.child_by_field_name("function")?;
    if callee.kind() != "member_expression" {
        return None;
    }
    let prop = callee.child_by_field_name("property")?;
    if prop.utf8_text(source).ok() != Some("addEventListener") {
        return None;
    }
    let args = node.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    let named: Vec<_> = args
        .named_children(&mut cursor)
        .filter(|c| c.kind() != "comment")
        .collect();
    if named.len() < 2 {
        return None;
    }
    let ev_node = named[0];
    if ev_node.kind() != "string" {
        return None;
    }
    let raw = ev_node.utf8_text(source).ok()?;
    let ev = raw.trim_matches(|c| c == '"' || c == '\'' || c == '`');
    if !HIGH_FREQ_EVENTS.contains(&ev) {
        return None;
    }
    let handler = named[1];
    if callback_contains_setstate(handler, source) {
        Some(node)
    } else {
        None
    }
}

fn check_jsx_handler<'a>(
    node: tree_sitter::Node<'a>,
    source: &'a [u8],
) -> Option<tree_sitter::Node<'a>> {
    if node.kind() != "jsx_attribute" {
        return None;
    }
    let name_node = node.child(0)?;
    let name = name_node.utf8_text(source).ok()?;
    if !HIGH_FREQ_JSX_PROPS.contains(&name) {
        return None;
    }
    // value is at child(2) (name, `=`, value).
    let value = node.child(2)?;
    if value.kind() != "jsx_expression" {
        return None;
    }
    if callback_contains_setstate(value, source) {
        Some(node)
    } else {
        None
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let _ = ctx;
    if let Some(hit) = check_addeventlistener(node, source) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &hit,
            super::META.id,
            "`setState` inside a high-frequency event listener (mousemove/scroll/...) — \
             use `useRef` for the transient value and only commit a render when needed."
                .into(),
            Severity::Warning,
        ));
        return;
    }
    if let Some(hit) = check_jsx_handler(node, source) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &hit,
            super::META.id,
            "`setState` inside a high-frequency JSX handler (onMouseMove/onScroll/...) — \
             use `useRef` for the transient value and only commit a render when needed."
                .into(),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_setstate_in_mousemove_listener() {
        let src = r#"
el.addEventListener("mousemove", (e) => { setX(e.clientX); });
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_setstate_in_onmousemove_jsx() {
        let src = r#"const v = <div onMouseMove={(e) => setX(e.clientX)} />;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_ref_in_mousemove() {
        let src = r#"
el.addEventListener("mousemove", (e) => { xRef.current = e.clientX; });
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_setstate_in_click() {
        let src = r#"const v = <div onClick={() => setX(1)} />;"#;
        assert!(run(src).is_empty());
    }
}
