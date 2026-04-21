//! rust-prefer-once-lock backend.
//!
//! Matches `lazy_static!` macro invocations and `once_cell::sync::{Lazy,OnceCell}`
//! generic type annotations via tree-sitter. `LazyLock`/`OnceLock` from
//! `std::sync` are the supported replacements since Rust 1.70 and carry
//! none of the third-party weight.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let msg = "Use `std::sync::LazyLock` or `OnceLock` (stable since Rust 1.70) instead of `lazy_static!` or `once_cell`.";

    if node.kind() == "macro_invocation" {
        if let Some(name_node) = node.child_by_field_name("macro")
            && name_node.utf8_text(source).unwrap_or("") == "lazy_static"
        {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                msg.into(),
                Severity::Warning,
            ));
        }
        return;
    }

    if node.kind() == "generic_type" {
        let Some(type_node) = node.child_by_field_name("type") else { return; };
        let type_text = type_node.utf8_text(source).unwrap_or("");
        let is_target = matches!(type_text, "Lazy" | "OnceCell")
            || type_text == "once_cell::sync::Lazy"
            || type_text == "once_cell::sync::OnceCell";
        if is_target {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                msg.into(),
                Severity::Warning,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(s, &Check)
    }

    #[test]
    fn flags_lazy_static_macro() {
        assert_eq!(run("lazy_static! { static ref FOO: String = String::new(); }").len(), 1);
    }

    #[test]
    fn flags_once_cell_lazy() {
        assert_eq!(run("static FOO: once_cell::sync::Lazy<String> = once_cell::sync::Lazy::new(|| compute());").len(), 1);
    }

    #[test]
    fn allows_std_once_lock() {
        assert!(run("static FOO: std::sync::OnceLock<String> = std::sync::OnceLock::new();").is_empty());
    }

    #[test]
    fn allows_lazy_lock() {
        assert!(run("static FOO: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| compute());").is_empty());
    }
}
