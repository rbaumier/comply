//! rust-string-as-error backend.
//!
//! Walks every `generic_type` and flags `Result<_, String>` wherever the error
//! type is an unforced local choice — free-function return types, inherent-impl
//! methods, struct fields, type aliases. Suppressed in trait method signatures
//! (trait definitions and trait impls), where the error type is a fixed public
//! API contract the author can't change without breaking callers, and in free
//! functions wired as a clap custom `value_parser` (`#[arg(value_parser = …)]`
//! or `#[clap(value_parser = …)]`), where clap requires `fn(&str) -> Result<T, E>`
//! with `E: Into<Box<dyn Error + Send + Sync>>` and consumes the error internally,
//! so `String` is the idiomatic error and a structured error adds no value.
//! Also suppressed in test context (`#[test]` functions, `#[cfg(test)]` modules,
//! `tests/` integration files), where `Result<_, String>` is the idiomatic
//! lightweight error-propagation pattern — tests only display the error message,
//! never pattern-match it, so a structured error enum adds no value.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{is_in_test_context, is_under_tests_dir, result_error_type};

crate::ast_check! { on ["generic_type"] => |node, source, ctx, diagnostics|
    let Some(err_type) = result_error_type(node, source) else {
        return;
    };
    let Ok(err_text) = err_type.utf8_text(source) else {
        return;
    };
    if err_text.trim() != "String" {
        return;
    }
    // `Result<_, String>` is the idiomatic lightweight error-propagation pattern
    // in test code: tests return `Result` so the body can use `?`, and a `String`
    // error is fine there since the harness only displays it. The typed-error
    // guidance applies to library/production code, not tests.
    if is_in_test_context(node, source) || is_under_tests_dir(ctx.path) {
        return;
    }
    // A `Result<_, String>` in a trait method signature is a public API contract:
    // the trait fixes the error type and every impl must conform — neither can
    // switch to a structured error unilaterally. Flag String-as-error only where it
    // is an unforced local choice (free/inherent functions, struct fields, aliases).
    if crate::rules::rust_helpers::is_in_trait_impl(node)
        || crate::rules::rust_helpers::is_in_trait_definition(node)
    {
        return;
    }
    if is_in_clap_value_parser_fn(node, source) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "rust-string-as-error".into(),
        message: "`Result<_, String>` is stringly-typed — callers can't \
                  pattern-match failure modes. Define a proper error enum \
                  (use `thiserror::Error`)."
            .into(),
        severity: Severity::Warning,
        span: None,
    });
}

/// True when `source` wires `fn_name` as a clap value parser via a
/// `value_parser = <fn_name>` attribute argument (`#[arg(value_parser = …)]`
/// or `#[clap(value_parser = …)]`). Tolerates surrounding whitespace and
/// requires `fn_name` to match as a whole identifier (so `value_parser =
/// parse_dir_2` does not match `parse_dir`).
fn source_wires_value_parser(source: &str, fn_name: &str) -> bool {
    for (idx, _) in source.match_indices("value_parser") {
        let rest = source[idx + "value_parser".len()..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(after) = rest.strip_prefix(fn_name) else {
            continue;
        };
        if after
            .bytes()
            .next()
            .is_none_or(|b| !b.is_ascii_alphanumeric() && b != b'_')
        {
            return true;
        }
    }
    false
}

/// True when the `Result<_, String>` node sits inside a free function that is
/// wired as a clap custom `value_parser`. clap requires `fn(&str) -> Result<T, E>`
/// with `E: Into<Box<dyn Error + Send + Sync>>`; `String` is the idiomatic `E`
/// and clap internals consume the error, so a structured error is unwarranted.
fn is_in_clap_value_parser_fn(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "function_item" {
            let Some(name) = parent
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
            else {
                return false;
            };
            let Ok(src) = std::str::from_utf8(source) else {
                return false;
            };
            return source_wires_value_parser(src, name);
        }
        cur = parent;
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

    #[test]
    fn flags_result_string_error() {
        assert_eq!(run_on("fn f() -> Result<i32, String> { Ok(0) }").len(), 1);
    }

    #[test]
    fn allows_result_string_in_test_fn() {
        // `Result<_, String>` is the idiomatic lightweight error-propagation
        // pattern in `#[test]` functions — the harness only displays the error.
        let src = "#[test]\nfn t() -> Result<(), String> { Ok(()) }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_result_string_in_cfg_test_module() {
        // A `#[cfg(test)]` module is test-only code.
        let src = "#[cfg(test)]\nmod tests { fn f() -> Result<i32, String> { Ok(0) } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_result_string_in_tests_dir() {
        // A file under `tests/` is integration-test infrastructure.
        let src = "fn f() -> Result<(), String> { Ok(()) }";
        assert!(
            crate::rules::test_helpers::run_rule(&Check, src, "tests/all/emplace.rs").is_empty()
        );
    }

    #[test]
    fn flags_result_string_in_production() {
        // Outside test context the typed-error guidance still applies.
        assert_eq!(run_on("fn f() -> Result<i32, String> { Ok(0) }").len(), 1);
    }

    #[test]
    fn allows_result_with_real_error_type() {
        assert!(run_on("fn f() -> Result<i32, MyError> { Ok(0) }").is_empty());
    }

    #[test]
    fn allows_result_unit_error() {
        // Unit-error is a different rule (`rust-unit-error-result`).
        // This rule only flags String — keep concerns separate.
        assert!(run_on("fn f() -> Result<i32, ()> { Ok(0) }").is_empty());
    }

    #[test]
    fn allows_result_string_in_trait_definition() {
        // The trait fixes the error type as part of its public API contract.
        let src = "pub trait T { fn f(&self) -> Result<i32, String>; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_result_string_in_trait_impl() {
        // A conforming impl can't change the contract unilaterally.
        let src = "impl T for S { fn f(&self) -> Result<i32, String> { Ok(0) } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_result_string_in_inherent_impl() {
        // No trait contract — the author chose `String` freely.
        let src = "impl S { fn f(&self) -> Result<i32, String> { Ok(0) } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_result_string_in_struct_field() {
        // A struct field is not a trait method signature.
        assert_eq!(run_on("struct S { e: Result<i32, String> }").len(), 1);
    }

    #[test]
    fn allows_result_string_in_clap_value_parser_fn() {
        // clap's custom value_parser requires `fn(&str) -> Result<T, E>`;
        // `String` is the idiomatic error and clap consumes it internally.
        let src = "#[derive(Parser)]\n\
                   struct Args { #[arg(value_parser = parse_dir)] dir: PathBuf }\n\
                   fn parse_dir(dir: &str) -> Result<PathBuf, String> { Ok(PathBuf::from(dir)) }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_result_string_in_clap_value_parser_fn_clap_spelling() {
        // The `#[clap(value_parser = …)]` spelling is equally a value-parser wiring.
        let src = "#[derive(Parser)]\n\
                   struct Args { #[clap(value_parser = parse_dir)] dir: PathBuf }\n\
                   fn parse_dir(dir: &str) -> Result<PathBuf, String> { Ok(PathBuf::from(dir)) }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_result_string_fn_without_value_parser_wiring() {
        // No `value_parser` wiring anywhere — the author chose `String` freely.
        assert_eq!(run_on("fn f() -> Result<i32, String> { Ok(0) }").len(), 1);
    }

    #[test]
    fn flags_value_parser_word_boundary_different_fn() {
        // `value_parser = parse_dir_2` must not word-match `parse_dir`, so the
        // unwired `parse_dir` is still flagged.
        let src = "#[derive(Parser)]\n\
                   struct Args { #[arg(value_parser = parse_dir_2)] dir: PathBuf }\n\
                   fn parse_dir(d: &str) -> Result<PathBuf, String> { Ok(PathBuf::from(d)) }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_struct_field_even_with_value_parser_elsewhere() {
        // A struct field has no enclosing function, so the value-parser
        // exemption can't apply even when wiring exists elsewhere in the file.
        let src = "struct S { e: Result<i32, String> }\n\
                   #[arg(value_parser = something)]\n\
                   fn g() {}";
        assert_eq!(run_on(src).len(), 1);
    }
}
