//! rust-no-empty-test-fn backend.
//!
//! Walks `function_item` nodes whose preceding `attribute_item`
//! sibling carries `#[test]` and whose body block has zero named
//! children (no statements, no expressions). The body still has
//! the `{` and `}` punctuation tokens, but those are anonymous
//! children — `named_child_count() == 0` is the right check.
//!
//! An empty test that also carries a `#[cfg(...)]` configuration
//! attribute is exempt: it is conditionally compiled, so the empty
//! body is intentional — when the gate is active the test compiles,
//! verifying the feature-gated code/macros compile (the compile
//! phase is the test).
//!
//! Empty test fns inside a trybuild/ui_test compile-fail fixture are
//! also exempt. Those fixtures hold intentionally-malformed test
//! attributes (e.g. `#[tokio::test(flavor = 123)]`) whose only job is
//! to make the compiler emit a diagnostic; they are never executed, so
//! the empty body is irrelevant. A fixture is recognised by a sibling
//! expected-output file (`<stem>.stderr` / `<stem>.stdout`) — the
//! canonical trybuild signal — or, when no sibling is present, by the
//! conventional fixture directory name (`ui` / `fail` / `compile-fail`
//! under a `tests` or `tests-build` ancestor).

use std::path::Path;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["function_item"];

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
        if !has_test_attribute(node, source_bytes) {
            return;
        }
        if has_cfg_attribute(node, source_bytes) {
            return;
        }
        if is_compile_fail_fixture(ctx.path) {
            return;
        }
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        if body.named_child_count() != 0 {
            return;
        }
        let name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source_bytes).ok())
            .unwrap_or("test");
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-no-empty-test-fn".into(),
            message: format!(
                "`#[test] fn {name}` has an empty body — it always \
                 passes without exercising any code. Fill it in or \
                 delete it."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

fn has_test_attribute(item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        if s.kind() != "attribute_item" {
            break;
        }
        if let Ok(text) = s.utf8_text(source)
            && (text.contains("#[test]")
                || text.contains("::test]")   // #[tokio::test], #[actix_rt::test], …
                || text.contains("::test("))  // #[tokio::test(flavor = "multi_thread")], …
        {
            return true;
        }
        sibling = s.prev_named_sibling();
    }
    false
}

/// True if the function carries a `#[cfg(...)]` configuration-predicate
/// attribute as a preceding `attribute_item` sibling. Such a test is
/// conditionally compiled: when the predicate is active it must compile,
/// so an empty body is an intentional compile-time check, not a stub.
fn has_cfg_attribute(item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        if s.kind() != "attribute_item" {
            break;
        }
        if let Ok(text) = s.utf8_text(source)
            && text.contains("cfg(")
        {
            return true;
        }
        sibling = s.prev_named_sibling();
    }
    false
}

/// True if `path` is a trybuild/ui_test compile-fail fixture: a `.rs` file
/// whose functions are meant to fail to compile, never to run. Two signals,
/// either of which is sufficient:
///
/// 1. A sibling expected-output file (`<stem>.stderr` or `<stem>.stdout`)
///    sits next to it — the canonical, framework-agnostic trybuild signal.
/// 2. The file lives in a conventional fixture directory (`ui`, `fail`, or
///    `compile-fail`) nested under a `tests` or `tests-build` ancestor.
///
/// The directory convention alone is deliberately narrow: ordinary empty
/// `#[test]` fns under `tests/` (without a `ui`/`fail`/`compile-fail`
/// segment) are still flagged.
fn is_compile_fail_fixture(path: &Path) -> bool {
    has_expected_output_sibling(path) || is_in_fixture_dir(path)
}

/// True if a `<stem>.stderr` or `<stem>.stdout` file exists alongside `path`.
fn has_expected_output_sibling(path: &Path) -> bool {
    ["stderr", "stdout"]
        .iter()
        .any(|ext| path.with_extension(ext).exists())
}

/// True if `path` contains a `ui` / `fail` / `compile-fail` segment that is
/// nested under a `tests` or `tests-build` ancestor segment.
fn is_in_fixture_dir(path: &Path) -> bool {
    let mut seen_tests_root = false;
    for segment in path.iter() {
        let Some(segment) = segment.to_str() else {
            continue;
        };
        if segment == "tests" || segment == "tests-build" {
            seen_tests_root = true;
        } else if seen_tests_root
            && matches!(segment, "ui" | "fail" | "compile-fail")
        {
            return true;
        }
    }
    false
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

    fn run_at(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
    }

    #[test]
    fn flags_empty_test_fn() {
        let source = "#[test]\nfn it_works() {}";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_test_with_assertion() {
        let source = "#[test]\nfn it_works() { assert_eq!(1 + 1, 2); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_empty_non_test_fn() {
        // Empty non-test fns are someone else's problem (no_empty_function rule).
        assert!(run_on("fn placeholder() {}").is_empty());
    }

    #[test]
    fn flags_empty_tokio_test_fn() {
        let src = "#[tokio::test]\nasync fn it_works() {}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_feature_gated_empty_test_as_compile_check() {
        // helix-lsp-types: an empty `#[test]` carrying a `#[cfg(feature)]`
        // gate is a compile-time check, not a stub (Closes #1462).
        let src = "#[test]\n#[cfg(feature = \"proposed\")]\nfn check_proposed_macro_definitions() {}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_empty_test_without_cfg() {
        // Negative-space guard: a plain empty `#[test]` with no cfg gate
        // must still fire — only the conditionally-compiled case is exempt.
        let src = "#[test]\nfn check_proposed_macro_definitions() {}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_empty_test_in_fixture_dir() {
        // tokio: compile-fail stubs in tests-build/tests/fail/ are never run;
        // the malformed attribute is the test, the empty body is irrelevant
        // (Closes #1447).
        let src = "#[tokio::test(flavor = 123)]\nasync fn test_flavor_not_string() {}";
        let path = "tests-build/tests/fail/macros_invalid_input.rs";
        assert!(run_at(src, path).is_empty());
    }

    #[test]
    fn allows_empty_test_in_ui_dir() {
        let src = "#[test]\nfn it_works() {}";
        assert!(run_at(src, "tests/ui/empty_test.rs").is_empty());
    }

    #[test]
    fn allows_empty_test_with_stderr_sibling() {
        // The canonical trybuild signal: a `<stem>.stderr` next to the fixture.
        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = dir.path().join("macros_invalid_input.rs");
        std::fs::write(fixture.with_extension("stderr"), "error[E0277]: ...")
            .expect("write stderr sibling");
        let src = "#[tokio::test(foo)]\nasync fn test_attr_has_args() {}";
        assert!(run_at(src, fixture.to_str().expect("utf8 path")).is_empty());
    }

    #[test]
    fn flags_empty_test_in_plain_tests_dir() {
        // Negative-space guard: an empty `#[test]` in an ordinary `tests/`
        // file (no ui/fail/compile-fail segment, no sibling) is still flagged.
        let src = "#[test]\nfn it_works() {}";
        assert_eq!(run_at(src, "tests/integration.rs").len(), 1);
    }
}
