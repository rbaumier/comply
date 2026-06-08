//! rust-no-dbg-macro backend.
//!
//! Walks `macro_invocation` nodes and flags any whose macro name is
//! `dbg`. We do NOT exempt tests — even in tests, `dbg!` is debug
//! scaffolding that should be removed once the bug is found.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

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
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-no-dbg-macro".into(),
            message: "`dbg!()` is a debugging aid — remove before \
                      committing. For permanent observability use \
                      `tracing::debug!` with structured fields."
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
}
