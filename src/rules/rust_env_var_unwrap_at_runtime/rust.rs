//! rust-env-var-unwrap-at-runtime backend.
//!
//! Walk every `call_expression` matching `<recv>.unwrap()` (or `.expect(...)`)
//! whose receiver is itself a `call_expression` to `env::var(...)` /
//! `std::env::var(...)`.
//! Three places may read env vars without a graceful fallback.
//! The first is code outside the release, per `is_outside_release`.
//! The others are `fn main` and a Cargo build script.
//! A build script runs at build time (single-threaded, with Cargo-set env
//! vars guaranteed present), so panicking on a missing env var there
//! correctly fails the build rather than degrading a running service.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::{is_in_fn_main, is_outside_release};

const KINDS: &[&str] = &["call_expression"];

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
        let source = ctx.source.as_bytes();
        let Some(function) = node.child_by_field_name("function") else {
            return;
        };
        if function.kind() != "field_expression" {
            return;
        }
        let Some(field) = function.child_by_field_name("field") else {
            return;
        };
        let Ok(method) = field.utf8_text(source) else {
            return;
        };
        if method != "unwrap" && method != "expect" {
            return;
        }
        let Some(receiver) = function.child_by_field_name("value") else {
            return;
        };
        if !is_env_var_call(receiver, source) {
            return;
        }
        let startup_code = is_in_fn_main(node, source)
            || crate::rules::path_utils::is_rust_build_script(ctx.path);
        if startup_code || is_outside_release(node, ctx) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            "rust-env-var-unwrap-at-runtime",
            format!(
                "`env::var(\"…\").{method}()` panics on missing env var. \
                 Read the variable in `main` (or a config bootstrap) and \
                 pass it through; in business logic, return a typed error."
            ),
            Severity::Error,
        ));
    }
}

fn is_env_var_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(function) = node.child_by_field_name("function") else {
        return false;
    };
    let text = match function.utf8_text(source) {
        Ok(t) => t,
        Err(_) => return false,
    };
    text == "env::var" || text == "std::env::var" || text == "::std::env::var"
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
    fn flags_env_var_unwrap_in_business_fn() {
        let src = r#"fn handler() { let url = std::env::var("URL").unwrap(); }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_env_var_expect_in_business_fn() {
        let src = r#"fn handler() { let url = env::var("URL").expect("URL set"); }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_env_var_unwrap_in_main() {
        let src = r#"fn main() { let url = std::env::var("URL").unwrap(); }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_env_var_unwrap_in_test() {
        let src = r#"#[test]
fn t() { let url = std::env::var("URL").unwrap(); }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_env_var_in_tests_dir_helper() {
        let src = r#"pub fn setup() { env::var("PATH").expect("PATH not set"); }"#;
        let run = |path| {
            crate::rules::test_helpers::run_rule_with_cargo(
                &Check,
                crate::rules::test_helpers::LIB_CARGO_TOML,
                src,
                path,
            )
        };
        assert!(run("tests/utils/mocked_pagers.rs").is_empty());
        assert!(run("examples/pager.rs").is_empty());
        assert_eq!(run("src/utils/pager_test.rs").len(), 1);
    }

    /// A crate named after tests ships to its dependents.
    /// No `cfg(test)` or Cargo target keeps it out.
    #[test]
    fn flags_env_var_in_a_crate_named_after_tests() {
        let src = r#"pub fn setup() { let r = std::env::var("NEXTEST_WORKSPACE_ROOT").expect("set"); }"#;
        for crate_dir in ["integration-tests/foo", "test-helpers"] {
            let diagnostics = crate::rules::test_helpers::run_rule_in_module_tree(
                &Check,
                &[
                    (&format!("{crate_dir}/Cargo.toml"), crate::rules::test_helpers::LIB_CARGO_TOML),
                    (&format!("{crate_dir}/src/lib.rs"), src),
                ],
            );
            assert_eq!(diagnostics.len(), 1, "{crate_dir}");
        }
    }

    #[test]
    fn flags_env_var_unwrap_in_latest_dir() {
        // `latest` contains `test` only as a non-delimited substring — it is a
        // production directory, not test infrastructure.
        let src = r#"fn load() { let url = std::env::var("URL").unwrap(); }"#;
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, src, "src/latest/v.rs").len(),
            1
        );
    }

    #[test]
    fn allows_env_var_with_unwrap_or() {
        let src = r#"fn handler() { let url = env::var("URL").unwrap_or_default(); }"#;
        // unwrap_or_default is a graceful fallback, not a panic — but our
        // rule only matches the literal `unwrap` / `expect` methods.
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_env_var_unwrap_in_build_script_helper() {
        let src = r#"fn helper() { let root = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()); }"#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "build.rs").is_empty());
    }

    #[test]
    fn allows_env_var_expect_in_build_script_helper() {
        let src = r#"fn helper() { let o = std::env::var("OUT_DIR").expect("set by cargo"); }"#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "build.rs").is_empty());
    }

    #[test]
    fn allows_custom_env_var_in_build_script() {
        let src = r#"fn helper() { let x = std::env::var("MY_CUSTOM").unwrap(); }"#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "build.rs").is_empty());
    }

    #[test]
    fn flags_env_var_unwrap_in_non_build_script_path() {
        let src = r#"fn load() { let url = std::env::var("DATABASE_URL").unwrap(); }"#;
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, src, "src/lib.rs").len(),
            1
        );
    }
}
