//! rust-thiserror-for-lib backend.
//!
//! Skips `main.rs` / `src/bin/` (application crates) and any file that
//! already mentions `thiserror`. In what remains, flags `enum_item`
//! declarations that are `pub` and whose name contains `Error` — the
//! signal that this is a library-facing error type which should derive
//! `thiserror::Error` rather than hand-roll `Display`/`Error`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "enum_item" { return; }

    let path_str = ctx.path.to_string_lossy();
    if path_str.contains("main.rs") || path_str.contains("src/bin/") { return; }
    if ctx.source.contains("thiserror") { return; }

    if !is_pub(node, source) { return; }

    let Some(name) = node.child_by_field_name("name") else { return; };
    let Ok(name_text) = name.utf8_text(source) else { return; };
    if !name_text.contains("Error") { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Use `#[derive(thiserror::Error)]` for library error types — avoids boilerplate `Display` impls.".into(),
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
        crate::rules::test_helpers::run_rust_with_path(s, &Check, "src/error.rs")
    }

    #[test]
    fn flags_pub_enum_error_without_thiserror() {
        assert_eq!(run("pub enum MyError { NotFound, Unauthorized }").len(), 1);
    }

    #[test]
    fn allows_enum_with_thiserror() {
        assert!(run("#[derive(thiserror::Error)]\npub enum MyError { #[error(\"not found\")] NotFound }").is_empty());
    }

    #[test]
    fn ignores_main_rs() {
        let diags = crate::rules::test_helpers::run_rust_with_path(
            "pub enum MyError { Fail }",
            &Check,
            "src/main.rs",
        );
        assert!(diags.is_empty());
    }
}
