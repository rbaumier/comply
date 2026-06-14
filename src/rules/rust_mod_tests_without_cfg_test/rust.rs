//! rust-mod-tests-without-cfg-test backend.
//!
//! Walks `mod_item` nodes whose name is `tests` (or `test`) and checks the
//! preceding `attribute_item` siblings for an attribute that activates the
//! `test` cfg — `#[cfg(test)]`, compound forms like `#[cfg(all(test, …))]`
//! and `#[cfg(any(test, …))]`, and `#[cfg_attr(test, …)]`. Flag if absent.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["mod_item"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let Ok(name) = name_node.utf8_text(source_bytes) else {
            return;
        };
        if name != "tests" && name != "test" {
            return;
        }
        if crate::rules::rust_helpers::has_test_attribute(node, source_bytes) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-mod-tests-without-cfg-test".into(),
            message: format!(
                "`mod {name}` is not gated by `#[cfg(test)]` — every \
                 test function will ship in the release binary. Add \
                 `#[cfg(test)]` immediately above the module declaration."
            ),
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
    fn flags_mod_tests_without_cfg() {
        assert_eq!(run_on("mod tests { #[test] fn t() {} }").len(), 1);
    }

    #[test]
    fn allows_mod_tests_with_cfg_test() {
        let source = "#[cfg(test)]\nmod tests { #[test] fn t() {} }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_other_module() {
        assert!(run_on("mod helpers { fn h() {} }").is_empty());
    }

    #[test]
    fn allows_mod_tests_with_compound_cfg_test() {
        let cases = [
            "#[cfg(all(test, not(loom)))]\nmod tests { #[test] fn t() {} }",
            "#[cfg(any(test, feature = \"x\"))]\nmod tests { #[test] fn t() {} }",
            "#[cfg(all(test, target_has_atomic = \"64\"))]\nmod test { #[test] fn t() {} }",
        ];
        for source in cases {
            assert!(
                run_on(source).is_empty(),
                "should not flag compound cfg(test): {source}"
            );
        }
    }

    #[test]
    fn skips_cargo_integration_test_file() {
        // Under a `tests/` dir the whole file is compiled test-only — an inner
        // `mod tests` without `#[cfg(test)]` must not be flagged (issue #1325).
        let diags = crate::rules::test_helpers::run_rule_gated(
            &Check,
            "mod tests { #[test] fn t() {} }",
            "tests/integration.rs",
        );
        assert!(diags.is_empty(), "must not flag mod tests in a tests/ integration file");
    }

    #[test]
    fn still_flags_mod_tests_in_src_file() {
        // A regular `src/*.rs` file is compiled into the production binary, so a
        // `mod tests` there genuinely needs `#[cfg(test)]` and stays flagged.
        let diags = crate::rules::test_helpers::run_rule_gated(
            &Check,
            "mod tests { #[test] fn t() {} }",
            "src/lib.rs",
        );
        assert_eq!(diags.len(), 1, "must still flag mod tests in a src/ file");
    }

    #[test]
    fn still_flags_mod_tests_with_non_test_cfg() {
        let cases = [
            "mod tests { #[test] fn t() {} }",
            "#[cfg(feature = \"x\")]\nmod tests { #[test] fn t() {} }",
            "#[cfg(not(test))]\nmod tests { #[test] fn t() {} }",
        ];
        for source in cases {
            assert_eq!(
                run_on(source).len(),
                1,
                "should still flag non-test-cfg mod tests: {source}"
            );
        }
    }
}
