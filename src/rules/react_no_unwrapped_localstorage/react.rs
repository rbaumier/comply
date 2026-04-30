//! AST backend for react-no-unwrapped-localstorage.
//!
//! Flags every `localStorage.<method>` member access whose ancestor
//! chain does not include a `try_statement`.

use crate::diagnostic::{Diagnostic, Severity};

fn is_localstorage_access(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    if node.kind() != "member_expression" {
        return false;
    }
    let Some(object) = node.child_by_field_name("object") else {
        return false;
    };
    object.kind() == "identifier" && object.utf8_text(source).ok() == Some("localStorage")
}

fn in_try_block(mut node: tree_sitter::Node<'_>) -> bool {
    while let Some(parent) = node.parent() {
        if parent.kind() == "try_statement" {
            // Make sure we are inside the `body` (the try block), not
            // inside the catch/finally (which are technically also
            // children of try_statement).
            if let Some(body) = parent.child_by_field_name("body") {
                let range = body.byte_range();
                let nrange = node.byte_range();
                if nrange.start >= range.start && nrange.end <= range.end {
                    return true;
                }
            }
        }
        node = parent;
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let _ = ctx;
    if !is_localstorage_access(node, source) {
        return;
    }
    if in_try_block(node) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`localStorage` access outside a `try`/`catch` — throws in private-browsing mode, \
         SSR, or on quota errors. Wrap in `try { ... } catch { ... }`."
            .into(),
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
    fn flags_unwrapped_getitem() {
        let src = r#"const v = localStorage.getItem("k");"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_unwrapped_setitem() {
        let src = r#"localStorage.setItem("k", "v");"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_wrapped_access() {
        let src = r#"try { localStorage.setItem("k", "v"); } catch (e) {}"#;
        assert!(run(src).is_empty());
    }
}
