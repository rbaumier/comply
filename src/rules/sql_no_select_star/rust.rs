//! sql-no-select-star — Rust backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::is_in_test_context;
use crate::rules::sql_helpers::RUST_STRING_KINDS;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(RUST_STRING_KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Ok(text) = node.utf8_text(source_bytes) else {
            return;
        };
        if !super::contains_select_star(text) {
            return;
        }
        // `SELECT *` inside a `#[cfg(test)]` module or `#[test]` fn is a query
        // fixture fed to the code under test, never run against a database, so
        // the bandwidth/covering-index rationale does not apply.
        if is_in_test_context(node, source_bytes) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "`SELECT *` wastes bandwidth — list columns explicitly so the \
             API contract is visible and covering indexes can work."
                .into(),
            Severity::Warning,
        ));
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.rs")
    }

    #[test]
    fn flags_select_star() {
        let src = r#"fn f() { let q = "SELECT * FROM users"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_explicit_columns() {
        let src = r#"fn f() { let q = "SELECT id, name FROM users"; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn repro_7840_select_star_in_cfg_test_module_not_flagged() {
        // Issue #7840 (spiceai nsql/mod.rs): a `SELECT *` string inside a
        // `#[cfg(test)] mod tests` block is a query fixture fed to the code under
        // test (an NL2SQL module), never run against a database, so the
        // bandwidth/covering-index rationale does not apply.
        let src = "#[cfg(test)]\n\
                   mod tests {\n\
                       #[test]\n\
                       fn f() {\n\
                           let _ = \"SELECT * FROM t\";\n\
                       }\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn repro_7840_select_star_in_test_fn_not_flagged() {
        // A `#[test]` function is equally exempt.
        let src = r#"#[test] fn t() { let _ = "SELECT * FROM t"; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn repro_7840_select_star_in_production_fn_still_flagged() {
        // Negative space: a `SELECT *` in a production fn outside any test
        // context keeps flagging, so the guard is scope-bound, not a blanket
        // silence.
        let src = r#"fn query() -> &'static str { "SELECT * FROM users" }"#;
        assert_eq!(run(src).len(), 1);
    }
}
