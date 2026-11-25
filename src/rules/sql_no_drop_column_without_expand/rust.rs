//! sql-no-drop-column-without-expand — Rust backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::is_test_only;
use crate::rules::sql_helpers::{RUST_STRING_KINDS, is_sql_ddl};
use crate::rules::walker::collect_nodes_of_kinds;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        if super::file_marks_deprecation(ctx.source) {
            return Vec::new();
        }
        let source_bytes = ctx.source.as_bytes();
        collect_nodes_of_kinds(tree, RUST_STRING_KINDS)
            .into_iter()
            .filter(|node| {
                node.utf8_text(source_bytes)
                    .is_ok_and(|text| is_sql_ddl(text) && super::sql_drops_column(text))
            })
            // A migration in a test is a fixture, never deployed.
            .filter(|node| !is_test_only(*node, ctx))
            .map(|node| {
                Diagnostic::at_node(
                    ctx.path,
                    &node,
                    super::META.id,
                    "DROP COLUMN without a prior deprecation release breaks running deploys — deprecate first, drop later.".into(),
                    Severity::Error,
                )
            })
            .collect()
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
    fn flags_bare_drop_column() {
        let src = r#"fn f() { let m = "ALTER TABLE account DROP COLUMN legacy_flag;"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_drop_column_in_test_code() {
        let src = "#[cfg(test)]\nmod tests {\n    const M: &str = \"ALTER TABLE account DROP COLUMN legacy_flag;\";\n}";
        assert!(run(src).is_empty());
        assert!(run_in_crate("tests/migrate.rs").is_empty());
    }

    #[test]
    fn flags_drop_column_in_a_migration_named_after_tests() {
        assert_eq!(
            run_in_crate("src/m20250101_000001_drop_test_mode.rs").len(),
            1
        );
        assert_eq!(run_in_crate("src/speed_test.rs").len(), 1);
    }

    fn run_in_crate(path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_cargo(
            &Check,
            crate::rules::test_helpers::LIB_CARGO_TOML,
            FLAGGED,
            path,
        )
    }

    const FLAGGED: &str = r#"fn f() { let m = "ALTER TABLE account DROP COLUMN legacy_flag;"; }"#;

    #[test]
    fn allows_with_deprecation_marker() {
        let src = "// expand-contract complete\nfn f() { let m = \"ALTER TABLE account DROP COLUMN legacy_flag;\"; }";
        assert!(run(src).is_empty());
    }
}
