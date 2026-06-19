use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Strips a leading zero-padded numeric ordering prefix (`<digits>_`) from a
/// stem, e.g. `0065_comment_newline` -> `comment_newline`. Such prefixes are a
/// widespread convention for lexicographically ordered fixtures, migrations and
/// parser test cases, and the remainder is what must satisfy the convention.
/// Returns the stem unchanged when there is no prefix or nothing follows it.
fn strip_ordering_prefix(stem: &str) -> &str {
    let digits = stem.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return stem;
    }
    match stem[digits..].strip_prefix('_') {
        Some(rest) if !rest.is_empty() => rest,
        _ => stem,
    }
}

/// Strips a leading `_` private-module prefix from a stem, e.g. `_features` ->
/// `features`. A leading underscore marks a Rust module as pseudo-private / not
/// part of the primary public API surface (compiled normally, often
/// `pub mod _features;`, used for rustdoc doc sub-modules); the remainder is
/// what must satisfy the convention. Scoped to `_` only, since `$` is not a
/// valid Rust identifier character.
fn strip_private_prefix(stem: &str) -> &str {
    stem.trim_start_matches('_')
}

fn is_snake_case(stem: &str) -> bool {
    if stem.is_empty() {
        return false;
    }
    let bytes = stem.as_bytes();
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    let mut prev_underscore = false;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'_' {
            if prev_underscore || i == 0 {
                return false;
            }
            prev_underscore = true;
        } else if b.is_ascii_lowercase() || b.is_ascii_digit() {
            prev_underscore = false;
        } else {
            return false;
        }
    }
    true
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let Some(file_name) = ctx.path.file_name().and_then(|s| s.to_str()) else {
            return Vec::new();
        };
        let stem = file_name.split('.').next().unwrap_or(file_name);
        if stem.is_empty() {
            return Vec::new();
        }
        // trybuild/rustc UI-test fixtures under `tests/ui/` conventionally use
        // kebab-case scenario names paired with sibling `.stderr` output.
        if crate::rules::path_utils::is_rust_ui_test_fixture(ctx.path) {
            return Vec::new();
        }
        // trybuild proc-macro fixtures under `tests/<suite>/pass|fail/` are
        // compiled as standalone crates whose kebab-case filename is the
        // test-scenario identifier.
        if crate::rules::path_utils::is_rust_trybuild_fixture(ctx.path) {
            return Vec::new();
        }
        // Cargo example targets under `examples/` compile to `--example <stem>`
        // binaries, so kebab-case stems are the standard Rust convention.
        if crate::rules::path_utils::is_cargo_example_path(ctx.path) {
            return Vec::new();
        }
        // Cargo binary targets directly in `src/bin/` produce a `--bin <stem>`
        // executable whose name is the file stem, so kebab-case stems are the
        // standard Rust convention; nested modules under `src/bin/` are not.
        if crate::rules::path_utils::is_cargo_bin_target_path(ctx.path) {
            return Vec::new();
        }
        if is_snake_case(strip_ordering_prefix(strip_private_prefix(stem))) {
            return Vec::new();
        }
        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: "filename-naming-convention".into(),
            message: format!("Filename `{file_name}` does not match snake_case convention."),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(path: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), ""))
    }

    #[test]
    fn allows_snake_case() {
        assert!(run("src/e2e_cli.rs").is_empty());
    }

    #[test]
    fn allows_single_word() {
        assert!(run("src/main.rs").is_empty());
    }

    #[test]
    fn allows_snake_case_with_digits() {
        assert!(run("src/oauth2_provider.rs").is_empty());
    }

    #[test]
    fn flags_kebab_case() {
        assert_eq!(run("src/e2e-cli.rs").len(), 1);
    }

    #[test]
    fn flags_camel_case() {
        assert_eq!(run("src/userProfile.rs").len(), 1);
    }

    #[test]
    fn flags_pascal_case() {
        assert_eq!(run("src/UserProfile.rs").len(), 1);
    }

    #[test]
    fn allows_trailing_underscore() {
        assert!(run("src/user_.rs").is_empty());
    }

    #[test]
    fn allows_keyword_avoidance_struct() {
        assert!(run("src/de/struct_.rs").is_empty());
    }

    #[test]
    fn allows_keyword_avoidance_type() {
        assert!(run("src/type_.rs").is_empty());
    }

    #[test]
    fn allows_keyword_avoidance_match() {
        assert!(run("src/match_.rs").is_empty());
    }

    #[test]
    fn flags_double_underscore() {
        assert_eq!(run("src/user__profile.rs").len(), 1);
    }

    #[test]
    fn allows_zero_padded_ordering_prefix() {
        assert!(run("crates/parser/test_data/parser/ok/0065_comment_newline.rs").is_empty());
    }

    #[test]
    fn flags_miscased_remainder_after_ordering_prefix() {
        assert_eq!(run("test_data/0065_CommentNewline.rs").len(), 1);
    }

    #[test]
    fn flags_non_prefixed_bad_name() {
        assert_eq!(run("src/BadName.rs").len(), 1);
    }

    #[test]
    fn flags_purely_numeric_stem() {
        assert_eq!(run("src/404.rs").len(), 1);
    }

    #[test]
    fn allows_kebab_case_in_tests_ui_fixture() {
        assert!(run("test_suite/tests/ui/enum-representation/untagged-struct.rs").is_empty());
    }

    #[test]
    fn still_flags_kebab_case_outside_tests_ui() {
        assert_eq!(run("src/my-module.rs").len(), 1);
    }

    #[test]
    fn allows_kebab_case_in_cargo_examples() {
        assert!(run("crates/searcher/examples/search-stdin.rs").is_empty());
    }

    #[test]
    fn allows_snake_case_in_cargo_examples() {
        assert!(run("examples/basic_search.rs").is_empty());
    }

    #[test]
    fn allows_kebab_case_cargo_bin_target() {
        assert!(run("src/bin/stdio-fixture.rs").is_empty());
        assert!(run("crates/searcher/src/bin/my-tool.rs").is_empty());
    }

    #[test]
    fn still_flags_kebab_case_module_nested_under_src_bin() {
        assert_eq!(run("src/bin/foo/my-helper.rs").len(), 1);
    }

    #[test]
    fn allows_kebab_case_in_trybuild_pass_fixture() {
        assert!(run("axum-macros/tests/from_ref/pass/reference-types.rs").is_empty());
    }

    #[test]
    fn allows_kebab_case_in_trybuild_fail_fixture() {
        assert!(run("axum-macros/tests/from_ref/fail/self-referential.rs").is_empty());
    }

    #[test]
    fn still_flags_kebab_case_in_trybuild_suite_without_pass_fail() {
        assert_eq!(run("axum-macros/tests/from_ref/reference-types.rs").len(), 1);
    }

    #[test]
    fn still_flags_kebab_case_outside_any_fixture() {
        assert_eq!(run("src/my-mod.rs").len(), 1);
    }

    #[test]
    fn allows_leading_underscore_private_module() {
        assert!(run("src/_features.rs").is_empty());
        assert!(run("src/_faq.rs").is_empty());
    }

    #[test]
    fn allows_leading_underscore_private_module_nested() {
        assert!(run("src/_derive/_tutorial.rs").is_empty());
    }

    #[test]
    fn still_flags_pascal_case_remainder_after_underscore() {
        assert_eq!(run("src/_FooBar.rs").len(), 1);
    }

    #[test]
    fn still_flags_pascal_case_without_underscore() {
        assert_eq!(run("src/MyModule.rs").len(), 1);
    }
}
