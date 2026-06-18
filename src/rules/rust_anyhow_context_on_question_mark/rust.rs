//! rust-anyhow-context-on-question-mark backend.
//!
//! Scoped to application crates (`main.rs`, `src/bin/`, `src/cli`)
//! so library code — which typically propagates typed errors with a
//! `thiserror` enum — isn't nagged to add `anyhow`-style context.
//! Walks every `try_expression` and flags the `?` only when the enclosing
//! function/closure returns an `anyhow`/`eyre` error type (where
//! `.context()` actually applies) and is not `fn main`, and the receiver
//! expression doesn't already chain `.context(` or `.with_context(`.
//! A typed-error-enum return (`Result<_, CliDiagnostic>`) is never flagged:
//! adding `.context()` there changes the error type and wouldn't compile.

use crate::diagnostic::{Diagnostic, Severity};

/// True when a function/closure signature's return type is one where
/// `anyhow::Context::context()` / `eyre::WrapErr` applies — `anyhow::Result<T>`,
/// `Result<_, anyhow::Error>`, `eyre::Result<T>`, `Result<_, eyre::Report>`.
/// A typed `thiserror` enum return (`Result<_, CliDiagnostic>`) does NOT match,
/// so `.context()` cannot legally be added and the `?` is not flagged.
fn return_type_context_applies(sig: &str) -> bool {
    sig.contains("anyhow::Result")
        || sig.contains("anyhow::Error")
        || sig.contains("eyre::Result")
        || sig.contains("eyre::Report")
}

crate::ast_check! { on ["try_expression"] => |node, source, ctx, diagnostics|
    let path_str = ctx.path.to_string_lossy();
    // `cli` must match as a real path segment — a `cli/` directory or a `cli.rs`
    // file — so library files like `src/client.rs` aren't mistaken for CLI code.
    let is_cli = ctx
        .path
        .components()
        .any(|c| matches!(c.as_os_str().to_str(), Some("cli" | "cli.rs")));
    if !path_str.contains("main.rs") && !path_str.contains("src/bin/") && !is_cli {
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
    let mut should_flag = false;
    while let Some(parent) = cur.parent() {
        match parent.kind() {
            "function_item" | "closure_expression" => {
                let body_start = parent
                    .child_by_field_name("body")
                    .map(|b| b.start_byte())
                    .unwrap_or(parent.end_byte());
                let sig = &source[parent.start_byte()..body_start];
                if let Ok(text) = std::str::from_utf8(sig) {
                    // A `?` propagates to the enclosing function/closure's return.
                    // Flag only when `.context()` applies there. `fn main` keeps
                    // its exemption (anyhow prints the full chain); a closure is
                    // never `fn main`.
                    should_flag = return_type_context_applies(text)
                        && !text.contains("fn main(");
                }
                break;
            }
            // Error type is undeterminable across an async block — skip.
            "async_block" => break,
            _ => {
                cur = parent;
            }
        }
    }
    if !should_flag {
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
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "src/main.rs")
    }

    fn run_lib(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.rs")
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

    #[test]
    fn ignores_client_file_not_matched_as_cli() {
        // `src/client.rs` is library code; "client" must not match the "cli" CLI segment.
        let src = r#"fn load() -> anyhow::Result<String> { let s = std::fs::read_to_string("x")?; Ok(s) }"#;
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "src/client.rs");
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_in_cli_module_file() {
        // a genuine `cli` path segment is still treated as a CLI entry point.
        let src = r#"fn load() -> anyhow::Result<String> { let s = std::fs::read_to_string("x")?; Ok(s) }"#;
        assert!(!crate::rules::test_helpers::run_rule(&Check, src, "src/cli/mod.rs").is_empty());
        assert!(!crate::rules::test_helpers::run_rule(&Check, src, "src/cli.rs").is_empty());
    }

    #[test]
    fn no_diagnostic_for_typed_enum_return_cli_diagnostic() {
        // `.context()` would change the error type and break the `?` From conversion,
        // so a typed thiserror enum return must not be flagged (issue #3789 repro).
        let src = r#"fn run_workspace(console: &mut C, command: B) -> Result<(), CliDiagnostic> { let r = Runtime::new()?; Ok(()) }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_diagnostic_for_typed_enum_return() {
        let src = r#"fn f() -> Result<(), MyError> { do_it()?; Ok(()) }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_bare_question_mark_in_eyre_result() {
        // eyre's WrapErr provides `.context()`, so the `?` is flagged like anyhow.
        let src = r#"fn f() -> eyre::Result<()> { x()?; Ok(()) }"#;
        assert!(!run(src).is_empty());
    }
}
