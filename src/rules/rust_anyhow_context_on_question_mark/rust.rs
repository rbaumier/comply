//! rust-anyhow-context-on-question-mark backend.
//!
//! Scoped to application crates (`main.rs`, `src/bin/`, `src/cli`)
//! so library code — which typically propagates typed errors with a
//! `thiserror` enum — isn't nagged to add `anyhow`-style context.
//! Walks every `try_expression` and flags the `?` when the receiver
//! expression doesn't already chain `.context(` or `.with_context(`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "try_expression" { return; }

    let path_str = ctx.path.to_string_lossy();
    if !path_str.contains("main.rs") && !path_str.contains("src/bin/") && !path_str.contains("src/cli") {
        return;
    }

    let inner = match node.named_child(0) {
        Some(i) => i,
        None => return,
    };
    let inner_text = inner.utf8_text(source).unwrap_or("");
    if inner_text.contains(".context(") || inner_text.contains(".with_context(") {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Add `.context(\"description\")` before `?` to give this error actionable context.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust_with_path(s, &Check, "src/main.rs")
    }

    fn run_lib(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(s, &Check)
    }

    #[test]
    fn flags_bare_question_mark_in_main() {
        let src = r#"fn load() -> anyhow::Result<String> { let s = std::fs::read_to_string("x")?; Ok(s) }"#;
        assert!(!run(src).is_empty());
    }

    #[test]
    fn allows_context_before_question_mark() {
        let src = r#"fn load() -> anyhow::Result<String> { let s = std::fs::read_to_string("x").context("reading file")?; Ok(s) }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_lib_files() {
        let src = r#"fn load() -> anyhow::Result<String> { let s = std::fs::read_to_string("x")?; Ok(s) }"#;
        assert!(run_lib(src).is_empty());
    }
}
