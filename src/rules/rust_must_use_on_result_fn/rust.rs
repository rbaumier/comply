//! rust-must-use-on-result-fn backend.
//!
//! Walks `function_item` nodes, keeps `pub` ones (matched via the
//! `visibility_modifier` child — tree-sitter-rust exposes visibility
//! as an anonymous child rather than a named field), filters to those
//! whose `return_type` text contains `Result<`, then scans the five
//! preceding lines for `#[must_use]`. If it's missing, flag.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["function_item"] => |node, source, ctx, diagnostics|
    if !is_pub(node, source) { return; }

    let ret = match node.child_by_field_name("return_type") {
        Some(r) => r,
        None => return,
    };
    if !ret.utf8_text(source).unwrap_or("").contains("Result<") { return; }

    let pos = node.start_position();
    let lines: Vec<&str> = ctx.source.lines().collect();
    let check_from = pos.row.saturating_sub(5);
    let preceding = &lines[check_from..pos.row];
    if preceding.iter().any(|l| l.contains("#[must_use]")) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Add `#[must_use]` — public functions returning `Result` must not allow callers to silently discard errors.".into(),
        Severity::Warning,
    ));
}

fn is_pub(item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = item.walk();
    for child in item.children(&mut cursor) {
        if child.kind() == "visibility_modifier"
            && let Ok(text) = child.utf8_text(source)
            && text.starts_with("pub")
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(s, &Check)
    }

    #[test]
    fn flags_pub_result_without_must_use() {
        assert_eq!(run("pub fn connect() -> Result<String, Error> { Ok(String::new()) }").len(), 1);
    }

    #[test]
    fn allows_must_use_attribute() {
        assert!(run("#[must_use]\npub fn connect() -> Result<String, Error> { Ok(String::new()) }").is_empty());
    }

    #[test]
    fn allows_private_fn() {
        assert!(run("fn connect() -> Result<String, Error> { Ok(String::new()) }").is_empty());
    }

    #[test]
    fn allows_non_result_return() {
        assert!(run("pub fn name() -> String { String::new() }").is_empty());
    }
}
