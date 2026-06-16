//! rust-no-panic-macros backend.
//!
//! Flags invocations of `panic!`, `todo!`, `unimplemented!`, and
//! `unreachable!` outside of test code. These macros all abort at
//! runtime — the opposite of what a production service should do.
//!
//! - `panic!` — turn it into a typed `Result` error.
//! - `todo!` / `unimplemented!` — placeholders that must not ship.
//! - `unreachable!` — only legitimate when marking a compiler-proven
//!   impossible state; document it with an `// Impossible: …` comment.
//!
//! Tests are exempted because panicking in a `#[test]` is a clean
//! failure mode. Same exemption logic as `rust-no-unwrap`. cargo-fuzz
//! targets (files under a `fuzz_targets/` directory) are also exempt:
//! in a libfuzzer-sys target, `panic!` is the deliberate
//! crash-signaling mechanism the fuzzer catches to report a found bug.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::{is_in_test_context, is_under_tests_dir};

const KINDS: &[&str] = &["macro_invocation"];

const BANNED_MACROS: &[&str] = &["panic", "todo", "unimplemented", "unreachable"];

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
        let Some(macro_name_node) = node.child_by_field_name("macro") else {
            return;
        };
        let Ok(macro_name) = macro_name_node.utf8_text(source_bytes) else {
            return;
        };
        if !BANNED_MACROS.contains(&macro_name) {
            return;
        }
        // Dual-read: the unit-test harness injects an empty default FileCtx, so
        // `in_fuzz_targets` is false in tests — fall back to the pure path
        // predicate, which reads `ctx.path` directly.
        if is_in_test_context(node, source_bytes)
            || is_under_tests_dir(ctx.path)
            || ctx.file.path_segments.in_fuzz_targets
            || crate::rules::path_utils::is_fuzz_targets_path(ctx.path)
        {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-no-panic-macros".into(),
            message: format!(
                "`{macro_name}!` aborts at runtime. Replace with a typed \
                 `Result` error. `todo!`/`unimplemented!` are placeholders \
                 that must not ship; `unreachable!` is only for \
                 compiler-proven impossible states with an `// Impossible:` \
                 comment. Tests are exempted."
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
    fn flags_panic_macro() {
        assert_eq!(run_on(r#"fn f() { panic!("boom"); }"#).len(), 1);
    }

    #[test]
    fn flags_todo_macro() {
        assert_eq!(run_on("fn f() { todo!(); }").len(), 1);
    }

    #[test]
    fn flags_unimplemented_macro() {
        assert_eq!(run_on("fn f() { unimplemented!(); }").len(), 1);
    }

    #[test]
    fn flags_unreachable_macro() {
        assert_eq!(run_on("fn f() { unreachable!(); }").len(), 1);
    }

    #[test]
    fn allows_panic_in_test_fn() {
        let source = "#[test]\nfn it_panics() { panic!(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_panic_in_cfg_test_module() {
        let source = "#[cfg(test)]\nmod tests { fn helper() { panic!(); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_println() {
        assert!(run_on(r#"fn f() { println!("hi"); }"#).is_empty());
    }

    #[test]
    fn allows_panic_in_tokio_test() {
        let source = "#[tokio::test]\nasync fn it_works() { panic!(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_panic_in_actix_rt_test() {
        let source = "#[actix_rt::test]\nasync fn it_works() { panic!(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_panic_in_tests_directory() {
        let source = "fn helper() { panic!(); }";
        assert!(crate::rules::test_helpers::run_rule(&Check, source, "tests/helpers.rs").is_empty());
    }

    #[test]
    fn allows_panic_in_fuzz_target() {
        let source = r#"fn run() { panic!("should be able to parse a printed value"); }"#;
        assert!(crate::rules::test_helpers::run_rule(
            &Check,
            source,
            "fuzz/fuzz_targets/rfc2822_parse.rs"
        )
        .is_empty());
    }

    #[test]
    fn flags_panic_in_regular_src() {
        let source = r#"fn f() { panic!("boom"); }"#;
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, source, "src/lib.rs").len(),
            1
        );
    }

    #[test]
    fn allows_panic_in_testing_rs() {
        let source = r#"pub fn h() { panic!("boom"); }"#;
        assert!(
            crate::rules::test_helpers::run_rule(&Check, source, "crates/foo/src/testing.rs")
                .is_empty()
        );
    }

    #[test]
    fn allows_panic_in_test_utils_rs() {
        let source = r#"pub fn h() { panic!("boom"); }"#;
        assert!(
            crate::rules::test_helpers::run_rule(&Check, source, "crates/foo/src/test_utils.rs")
                .is_empty()
        );
    }

    #[test]
    fn allows_panic_in_testutil_rs() {
        // ripgrep's crates/searcher/src/testutil.rs — the FP from #3282.
        let source = r#"pub fn h() { panic!("boom"); }"#;
        assert!(
            crate::rules::test_helpers::run_rule(
                &Check,
                source,
                "crates/searcher/src/testutil.rs"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_panic_under_testutil_dir() {
        let source = r#"pub fn h() { panic!("boom"); }"#;
        assert!(
            crate::rules::test_helpers::run_rule(
                &Check,
                source,
                "crates/foo/src/testutil/mod.rs"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_panic_under_property_tests_dir() {
        let source = r#"pub fn gen() { panic!("boom"); }"#;
        assert!(
            crate::rules::test_helpers::run_rule(
                &Check,
                source,
                "crates/foo/src/types/property_tests/setup.rs"
            )
            .is_empty()
        );
    }

    #[test]
    fn flags_panic_in_non_exact_testing_name() {
        let source = r#"pub fn m() { panic!("boom"); }"#;
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, source, "crates/foo/src/my_testing.rs")
                .len(),
            1
        );
    }

    #[test]
    fn flags_panic_in_non_exact_testing_dir() {
        let source = r#"pub fn tg() { panic!("boom"); }"#;
        assert_eq!(
            crate::rules::test_helpers::run_rule(
                &Check,
                source,
                "crates/foo/src/testingground/k.rs"
            )
            .len(),
            1
        );
    }
}
