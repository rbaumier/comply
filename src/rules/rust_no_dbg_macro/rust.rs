//! rust-no-dbg-macro backend.
//!
//! Walks `macro_invocation` nodes and flags any whose macro name is
//! `dbg` in production code. Test code is exempted: `dbg!()` inside a
//! `#[cfg(test)]`/`#[test]` context or under a `tests/` directory is
//! intentionally committed (e.g. snapshot-test harnesses print the value
//! under test) and never reaches a production binary.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::{is_in_test_context, is_under_tests_dir};

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
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-no-dbg-macro".into(),
            message: "`dbg!()` is a debugging aid — remove before \
                      committing. For permanent observability use \
                      `tracing::debug!` with structured fields. \
                      Tests are exempted."
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
}
