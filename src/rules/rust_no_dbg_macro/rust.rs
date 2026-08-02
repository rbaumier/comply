//! rust-no-dbg-macro backend.
//!
//! Walks `macro_invocation` nodes and flags any whose macro name is
//! `dbg` in production code. Test code is exempted: `dbg!()` inside a
//! `#[cfg(test)]`/`#[test]` context or under a `tests/` directory is
//! intentionally committed (e.g. snapshot-test harnesses print the value
//! under test) and never reaches a production binary.
//!
//! A `dbg!()` in the `then` branch of an environment-variable gate is exempt
//! as well: the gate is the author writing an opt-in debug mode, so the call
//! is a committed diagnostic rather than a leftover. The accepted gate shapes
//! are those of
//! [`is_under_env_var_gate`](crate::rules::rust_helpers::is_under_env_var_gate).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::{is_in_test_context, is_under_env_var_gate, is_under_tests_dir};

const KINDS: &[&str] = &["macro_invocation"];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(macro_node) = node.child_by_field_name("macro") else {
            return;
        };
        let Ok(name) = macro_node.utf8_text(source_bytes) else {
            return;
        };
        if name != "dbg" {
            return;
        }
        if is_in_test_context(node, source_bytes) || is_under_tests_dir(ctx.path) {
            return;
        }
        if is_under_env_var_gate(node, source_bytes) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-no-dbg-macro".into(),
            message: "`dbg!()` is a debugging aid — remove before \
                      committing. For permanent observability use \
                      `tracing::debug!` with structured fields. \
                      Tests and env-var-gated debug modes are exempted."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    fn run_with_path(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
    }

    #[test]
    fn flags_dbg_macro() {
        assert_eq!(run_on("fn f() { dbg!(x); }").len(), 1);
    }

    #[test]
    fn flags_dbg_in_let_binding() {
        assert_eq!(run_on("fn f() { let y = dbg!(compute()); }").len(), 1);
    }

    #[test]
    fn does_not_flag_println() {
        assert!(run_on(r#"fn f() { println!("hi"); }"#).is_empty());
    }

    #[test]
    fn allows_dbg_in_cfg_test_module() {
        let source = "#[cfg(test)]\nmod tests { fn helper() { dbg!(x); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_dbg_in_test_fn() {
        let source = "#[test]\nfn it_works() { dbg!(x); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_dbg_in_tests_directory() {
        let source = "fn t(toml: &str) { dbg!(toml); }";
        assert!(run_with_path(source, "crates/toml_edit/tests/compliance/invalid.rs").is_empty());
    }

    #[test]
    fn flags_dbg_in_cfg_not_test_module() {
        let source = "#[cfg(not(test))]\nmod prod { fn f() { dbg!(x); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    /// nushell's `StateWorkingSet::read_span`: the `dbg!()` calls run only
    /// when the consumer exports `MIETTE_DEBUG`, so the gate makes them a
    /// committed debug mode. Both the top-level call and the one nested in the
    /// loop are covered by the one binding.
    #[test]
    fn allows_dbg_under_env_var_bound_to_local() {
        let source = r#"
fn read_span(&self, span: &SourceSpan) -> Result<(), Error> {
    let debugging = std::env::var("MIETTE_DEBUG").is_ok();
    if debugging {
        let finding_span = "Finding span in StateWorkingSet";
        dbg!(finding_span, span);
    }
    for cached_file in self.files() {
        if debugging {
            dbg!(&filename, start, end);
        }
    }
    Ok(())
}
"#;
        assert!(run_on(source).is_empty());
    }

    /// `let mut` is the same binding — the `mut` is not part of the gate.
    #[test]
    fn allows_dbg_under_mut_env_var_local() {
        let source = r#"fn f() { let mut debugging = std::env::var("K").is_ok(); if debugging { dbg!(x); } }"#;
        assert!(run_on(source).is_empty());
    }

    /// A closure captures the enclosing binding, so the gate reaches inside it.
    #[test]
    fn allows_dbg_under_env_var_local_captured_by_closure() {
        let source = r#"fn f() { let debugging = std::env::var("K").is_ok(); items.for_each(|i| { if debugging { dbg!(i); } }); }"#;
        assert!(run_on(source).is_empty());
    }

    /// A closure parameter shadows the enclosing binding — the `dbg!()` runs on
    /// whatever the caller passes, so it is not gated.
    #[test]
    fn flags_dbg_when_closure_param_shadows_env_var_local() {
        let source = r#"fn f() { let debugging = std::env::var("K").is_ok(); run(|debugging: bool| { if debugging { dbg!(x); } }); }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    /// A `for` binder shadows the enclosing binding, so the loop body is not
    /// gated.
    #[test]
    fn flags_dbg_when_for_binder_shadows_env_var_local() {
        let source = r#"fn f() { let debugging = std::env::var("K").is_ok(); for debugging in flags { if debugging { dbg!(x); } } }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    /// A nested `fn` captures nothing from its parent, so the outer binding
    /// cannot gate its body.
    #[test]
    fn flags_dbg_in_nested_fn_with_same_named_param() {
        let source = r#"fn outer() { let debugging = std::env::var("K").is_ok(); fn inner(debugging: bool) { if debugging { dbg!(x); } } }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_dbg_under_inline_env_var_guard() {
        let source = r#"fn f() { if std::env::var("KEY").is_ok() { dbg!(x); } }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_dbg_under_inline_env_var_os_guard() {
        let source = r#"fn f() { if env::var_os("KEY").is_some() { dbg!(x); } }"#;
        assert!(run_on(source).is_empty());
    }

    /// The gate is the env-var check, not the name of the local it lands in:
    /// the same `if debugging { … }` over a plain bool stays flagged.
    #[test]
    fn flags_dbg_under_non_env_var_bool_local() {
        let source = "fn f() { let debugging = compute(); if debugging { dbg!(x); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    /// A later rebind to something else is the binding in force at the `if`,
    /// so the env-var check no longer gates the `dbg!()`.
    #[test]
    fn flags_dbg_when_env_var_local_is_shadowed() {
        let source = r#"
fn f() {
    let debugging = std::env::var("KEY").is_ok();
    let debugging = other_condition();
    if debugging { dbg!(x); }
}
"#;
        assert_eq!(run_on(source).len(), 1);
    }

    /// The `else` branch runs when the variable is *unset* — that is the
    /// unconditional path, not the opt-in one.
    #[test]
    fn flags_dbg_in_else_branch_of_env_var_guard() {
        let source = r#"fn f() { if std::env::var("KEY").is_ok() { g(); } else { dbg!(x); } }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_dbg_after_env_var_guard_block() {
        let source = r#"
fn f() {
    if std::env::var("KEY").is_ok() { g(); }
    dbg!(x);
}
"#;
        assert_eq!(run_on(source).len(), 1);
    }
}
