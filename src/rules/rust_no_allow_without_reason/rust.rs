//! Walks `attribute_item` nodes matching `#[allow(...)]`.
//! Flags when no comment exists on the same line or the line above.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::is_in_test_context;

crate::ast_check! { on ["attribute_item"] => |node, source, ctx, diagnostics|
    if ctx.file.path_segments.in_test_dir { return; }
    if is_in_test_context(node, source) { return; }

    let text = node.utf8_text(source).unwrap_or("");
    if !text.starts_with("#[allow(") && !text.starts_with("#[allow (") {
        return;
    }

    let row = node.start_position().row;

    let src_str = std::str::from_utf8(source).unwrap_or("");
    let lines: Vec<&str> = src_str.lines().collect();

    let has_inline_comment = lines.get(row).is_some_and(|l| {
        if let Some(pos) = l.find("//") {
            pos > l.find("#[allow").unwrap_or(0)
        } else {
            false
        }
    });

    let has_preceding_comment = row > 0 && lines.get(row - 1).is_some_and(|l| l.trim().starts_with("//"));

    if has_inline_comment || has_preceding_comment {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("`{text}` without justification — add a `//` comment explaining why."),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(s, &Check)
    }

    #[test]
    fn flags_bare_allow() {
        assert_eq!(run("#[allow(dead_code)]\nfn f() {}").len(), 1);
    }

    #[test]
    fn allows_with_inline_comment() {
        assert!(run("#[allow(dead_code)] // kept for FFI compat\nfn f() {}").is_empty());
    }

    #[test]
    fn allows_with_preceding_comment() {
        assert!(run("// mirrors std API naming\n#[allow(clippy::wrong_self_convention)]\nfn f() {}").is_empty());
    }

    #[test]
    fn ignores_in_test_context() {
        assert!(run("#[cfg(test)]\nmod tests {\n#[allow(unused)]\nfn f() {}\n}").is_empty());
    }

    #[test]
    fn ignores_non_allow_attributes() {
        assert!(run("#[derive(Debug)]\nstruct S;").is_empty());
    }
}
