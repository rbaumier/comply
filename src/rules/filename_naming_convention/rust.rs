use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

/// Rust backend for `filename-naming-convention`: flags `.rs` filenames whose
/// stem is not snake_case, after stripping a numeric ordering prefix
/// (`0065_`) and a leading `_` private-module marker. Cargo target paths
/// (`src/bin/`, `examples/`, `benches/`, files directly in `tests/`, and any
/// file declared as an explicit `[[bin]]` / `[[example]]` / `[[bench]]` /
/// `[[test]]` target via a `path = "..."` field), trybuild and rustc-UI fixture
/// paths are exempt because their kebab-case name
/// is the build-target / scenario identifier. Files carrying a machine-generated
/// marker near the top of their content are also exempt: the generator
/// dictates the name (e.g. wasm-bindgen WebIDL `gen_*.rs`).
#[derive(Debug)]
pub struct Check;

/// Number of leading lines scanned for a machine-generated marker. Generated
/// banners sit at the very top of the file, so a small window keeps the scan
/// cheap and avoids matching the marker text inside an unrelated string or
/// comment further down.
const GENERATED_MARKER_SCAN_LINES: usize = 15;

/// Returns `true` when the first few lines carry a machine-generated marker:
/// a blanket `#![allow(clippy::all)]` inner attribute (whitespace-tolerant,
/// since codegen may emit odd spacing), an `@generated` header, or a
/// `Code generated … DO NOT EDIT` banner. Such files take their name from the
/// generator, so the snake_case convention cannot apply.
fn is_generated_rust_source(source: &str) -> bool {
    source.lines().take(GENERATED_MARKER_SCAN_LINES).any(|line| {
        let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
        compact.contains("#![allow(clippy::all)]")
            || line.contains("@generated")
            || line.contains("DO NOT EDIT")
    })
}

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
        // Machine-generated Rust (wasm-bindgen WebIDL `gen_*.rs`, etc.) takes its
        // filename from the generator (the PascalCase suffix mirrors the source
        // interface name) and cannot be renamed. A blanket `#![allow(clippy::all)]`
        // inner attribute / `@generated` / `DO NOT EDIT` header marks such files.
        if is_generated_rust_source(ctx.source) {
            return Vec::new();
        }
        let Some(file_name) = ctx.path.file_name().and_then(|s| s.to_str()) else {
            return Vec::new();
        };
        let stem = file_name.split('.').next().unwrap_or(file_name);
        if stem.is_empty() {
            return Vec::new();
        }
        // rustc/compiletest fixtures under `tests/ui/` or `tests/compile-fail/`
        // conventionally use kebab-case scenario names paired with sibling
        // `.stderr` output.
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
        // Cargo benchmark targets directly in `benches/` are auto-discovered with
        // `--bench <stem>`, where the stem is a freely-chosen target name that
        // conventionally uses kebab-case; nested modules under `benches/` are not.
        if crate::rules::path_utils::is_cargo_bench_target_path(ctx.path) {
            return Vec::new();
        }
        // Cargo integration-test targets directly in `tests/` are auto-discovered
        // with `--test <stem>`, where the stem is a freely-chosen target name that
        // conventionally uses kebab-case; nested helper modules under `tests/`
        // (e.g. `tests/common/mod.rs`) are not.
        if crate::rules::path_utils::is_cargo_integration_test_target_path(ctx.path) {
            return Vec::new();
        }
        // Cargo targets declared with an explicit `path = "..."` in the nearest
        // `Cargo.toml` (`[[bin]]`/`[[example]]`/`[[bench]]`/`[[test]]`) may sit
        // anywhere in the crate, not just `src/bin/` or `examples/`. Their
        // filename is the author-declared build-target path and mirrors the
        // kebab-case binary name, so it is not a module name subject to the
        // snake_case convention.
        if ctx
            .project
            .nearest_cargo_manifest(ctx.path)
            .is_some_and(|m| m.declares_executable_at(ctx.path))
        {
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

    fn run_with_source(path: &str, source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), source))
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
    fn allows_kebab_case_in_tests_compile_fail_fixture_issue5153() {
        assert!(run("proptest-derive/tests/compile-fail/E0002-no-unions.rs").is_empty());
        assert!(run("proptest-derive/tests/compile_fail/no-arbitrary.rs").is_empty());
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
    fn allows_kebab_case_explicit_cargo_bin_target_with_custom_path() {
        // Regression #5180: a `[[bin]]` target declared with an explicit
        // `path =` pointing outside `src/bin/` (nextest's `cargo-nextest-dup`
        // helper) carries the kebab-case binary name as its filename. With a
        // real on-disk Cargo.toml + ProjectCtx the file is exempt, while an
        // ordinary kebab-case `src/` module in the same crate is still flagged.
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"helpers\"\nversion = \"0.1.0\"\n\n\
             [[bin]]\nname = \"cargo-nextest-dup\"\npath = \"test-helpers/cargo-nextest-dup.rs\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("test-helpers")).unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        let target = dir.path().join("test-helpers/cargo-nextest-dup.rs");
        let module = dir.path().join("src/my-module.rs");

        let project = crate::project::ProjectCtx::empty();
        let target_diags = Check.check(&CheckCtx::for_test_with_project(&target, "", &project));
        assert!(target_diags.is_empty());

        let module_diags = Check.check(&CheckCtx::for_test_with_project(&module, "", &project));
        assert_eq!(module_diags.len(), 1);
    }

    #[test]
    fn still_flags_kebab_case_module_nested_under_src_bin() {
        assert_eq!(run("src/bin/foo/my-helper.rs").len(), 1);
    }

    #[test]
    fn allows_kebab_case_cargo_bench_target() {
        assert!(run("fixtures/fixture-project/benches/my-bench.rs").is_empty());
        assert!(run("crates/searcher/benches/io.rs").is_empty());
    }

    #[test]
    fn still_flags_kebab_case_module_nested_under_benches() {
        assert_eq!(run("benches/my_bench/my-helper.rs").len(), 1);
    }

    #[test]
    fn allows_kebab_case_cargo_integration_test_target() {
        // The issue's exact reproducer path (indicatif): a file directly in
        // `tests/` is a Cargo integration-test target whose stem is the
        // freely-chosen target name.
        assert!(run("tests/multi-autodrop.rs").is_empty());
        // A workspace member's `tests/` is the same convention.
        assert!(run("crates/foo/tests/io.rs").is_empty());
    }

    #[test]
    fn still_flags_kebab_case_module_nested_under_tests() {
        // Nested helper modules under `tests/` are ordinary modules and must
        // still follow snake_case.
        assert_eq!(run("tests/common/foo-bar.rs").len(), 1);
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

    #[test]
    fn allows_wasm_bindgen_generated_pascal_suffix() {
        assert!(
            run_with_source(
                "crates/webidl-tests/src/features/gen_MixinFoo.rs",
                "#![allow(unused_imports)]\n#![allow(clippy::all)]\nuse super::*;\n",
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_generated_header_marker() {
        assert!(
            run_with_source("src/features/gen_Foo.rs", "// @generated\npub struct Foo;\n")
                .is_empty()
        );
    }

    #[test]
    fn allows_clippy_all_marker_with_codegen_spacing() {
        assert!(
            run_with_source(
                "src/features/gen_Bar.rs",
                "# ! [ allow ( clippy :: all ) ]\nuse super::*;\n",
            )
            .is_empty()
        );
    }

    #[test]
    fn still_flags_generated_name_without_marker() {
        assert_eq!(
            run_with_source(
                "crates/webidl-tests/src/features/gen_MixinFoo.rs",
                "pub struct MixinFoo;\n",
            )
            .len(),
            1
        );
    }

    #[test]
    fn still_flags_bad_name_without_marker_with_source() {
        assert_eq!(run_with_source("src/BadName.rs", "pub fn x() {}").len(), 1);
    }

    #[test]
    fn allows_snake_case_with_marker_absent() {
        assert!(run_with_source("src/foo_bar.rs", "pub fn x() {}").is_empty());
    }
}
