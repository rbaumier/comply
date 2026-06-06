//! rust-anyhow-context-on-question-mark backend.
//!
//! Scoped to application crates (`main.rs`, `src/bin/`, `src/cli`)
//! so library code — which typically propagates typed errors with a
//! `thiserror` enum — isn't nagged to add `anyhow`-style context.
//! Walks every `try_expression` and flags the `?` when the receiver
//! expression doesn't already chain `.context(` or `.with_context(`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["try_expression"] => |node, source, ctx, diagnostics|
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

    let mut cur = node;
    let mut in_main_anyhow = false;
    while let Some(parent) = cur.parent() {
        match parent.kind() {
            "function_item" => {
                let body_start = parent
                    .child_by_field_name("body")
                    .map(|b| b.start_byte())
                    .unwrap_or(parent.end_byte());
                let sig = &source[parent.start_byte()..body_start];
                if let Ok(text) = std::str::from_utf8(sig) {
                    in_main_anyhow =
                        text.contains("fn main(") && text.contains("anyhow::Result");
                }
                break;
            }
            "closure_expression" | "async_block" => break,
            _ => {
                cur = parent;
            }
        }
    }
    if in_main_anyhow {
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
    fn no_diagnostic_in_main_returning_anyhow_result() {
        // bare ? in fn main() -> anyhow::Result is idiomatic — anyhow prints the full error chain
        let src = r#"fn main() -> anyhow::Result<()> { let s = std::fs::read_to_string("x")?; Ok(()) }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_bare_question_mark_in_helper_returning_anyhow_result() {
        // non-main functions still need context
        let src = r#"fn helper() -> anyhow::Result<()> { let s = std::fs::read_to_string("x")?; Ok(()) }"#;
        assert!(!run(src).is_empty());
    }

    #[test]
    fn flags_question_mark_in_closure_inside_main_anyhow() {
        // closure boundary breaks the exemption — ? inside a closure is not in main's scope
        let src = r#"fn main() -> anyhow::Result<()> { let f = || -> anyhow::Result<()> { let s = std::fs::read_to_string("x")?; Ok(()) }; Ok(()) }"#;
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
