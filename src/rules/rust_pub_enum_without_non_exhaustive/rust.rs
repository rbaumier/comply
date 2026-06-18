//! rust-pub-enum-without-non-exhaustive backend.
//!
//! Walks `enum_item` nodes with `pub` visibility and scans the
//! preceding `attribute_item` siblings for `#[non_exhaustive]`. If
//! absent, flag.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::{is_in_test_context, is_under_tests_dir};
use std::path::{Path, PathBuf};

const KINDS: &[&str] = &["enum_item"];

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
        if !is_pub(node, source_bytes) {
            return;
        }
        if has_non_exhaustive(node, source_bytes) {
            return;
        }
        // C-ABI enum (`#[repr(C)]` or `#[repr(<int>)]`): the fixed, complete set
        // of integer-valued variants *is* the cross-language ABI contract the C
        // side switches on. `#[non_exhaustive]` is a Rust-only construct that C
        // cannot see and that contradicts the `#[repr(C)]` intent, so it is not
        // the right fix here.
        if has_c_repr(node, source_bytes) {
            return;
        }
        // Test-helper enum: `#[non_exhaustive]` would force wildcard match
        // arms in tests, defeating exhaustiveness checking. Not an external API.
        // A `pub enum` under a `tests/` directory (integration tests, fixtures)
        // is likewise never a SemVer-bound published surface.
        if is_in_test_context(node, source_bytes) || is_under_tests_dir(ctx.path) {
            return;
        }
        // Binary-only crate (no `[lib]` target): no external consumers, so
        // adding a variant is never a SemVer break.
        if ctx
            .project
            .nearest_cargo_manifest(ctx.path)
            .is_some_and(|m| m.is_binary_only())
        {
            return;
        }
        if is_internal_crate(ctx.path) {
            return;
        }
        let name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source_bytes).ok())
            .unwrap_or("Enum");
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-pub-enum-without-non-exhaustive".into(),
            message: format!(
                "`pub enum {name}` lacks `#[non_exhaustive]` — adding \
                 a new variant later becomes a SemVer-breaking change. \
                 Add the attribute to keep the API future-proof."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_internal_crate(path: &Path) -> bool {
    let Some(manifest) = nearest_manifest(path) else {
        return false;
    };
    // From here a `Cargo.toml` exists: the crate's publish status is what
    // decides whether its `pub enum`s are a SemVer-bound public API. The rule
    // must only flag when the crate is *provably* published, so any failure to
    // read or parse the manifest fails open to "internal" — notably TOML 1.1
    // multiline inline tables, which the `toml` 0.8 parser rejects.
    let Ok(content) = std::fs::read_to_string(&manifest) else {
        return true;
    };
    let Ok(value) = content.parse::<toml::Value>() else {
        return true;
    };

    if !value.get("package").is_some_and(toml::Value::is_table) {
        return true;
    }
    if publish_is_disabled(&value) {
        return true;
    }

    let Some(workspace_manifest) = nearest_workspace_manifest(path, &manifest) else {
        return false;
    };
    if workspace_manifest == manifest {
        return false;
    }

    !publish_is_explicitly_enabled(&value)
}

fn nearest_manifest(path: &Path) -> Option<PathBuf> {
    let mut dir = path.parent();
    while let Some(d) = dir {
        let cargo_toml = d.join("Cargo.toml");
        if cargo_toml.is_file() {
            return Some(cargo_toml);
        }
        dir = d.parent();
    }
    None
}

fn nearest_workspace_manifest(path: &Path, nearest: &Path) -> Option<PathBuf> {
    let mut dir = path.parent();
    while let Some(d) = dir {
        let cargo_toml = d.join("Cargo.toml");
        if cargo_toml != nearest
            && let Ok(content) = std::fs::read_to_string(&cargo_toml)
            && let Ok(value) = content.parse::<toml::Value>()
            && value.get("workspace").is_some_and(toml::Value::is_table)
        {
            return Some(cargo_toml);
        }
        dir = d.parent();
    }
    None
}

fn publish_is_disabled(value: &toml::Value) -> bool {
    let Some(publish) = value.get("package").and_then(|p| p.get("publish")) else {
        return false;
    };
    publish.as_bool() == Some(false) || publish.as_array().is_some_and(|items| items.is_empty())
}

fn publish_is_explicitly_enabled(value: &toml::Value) -> bool {
    let Some(publish) = value.get("package").and_then(|p| p.get("publish")) else {
        return false;
    };
    publish.as_bool() == Some(true)
        || publish
            .as_array()
            .is_some_and(|registries| !registries.is_empty())
}

fn is_pub(item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = item.walk();
    for child in item.children(&mut cursor) {
        if child.kind() == "visibility_modifier"
            && let Ok(text) = child.utf8_text(source)
        {
            return text == "pub";
        }
    }
    false
}

fn has_non_exhaustive(item: tree_sitter::Node, source: &[u8]) -> bool {
    // Walk every preceding sibling; keep going through attribute_item
    // and interleaved comment nodes (tree-sitter-rust inserts
    // `line_comment`/`block_comment` siblings for trailing `//` notes).
    // Without this, a comment between `#[non_exhaustive]` and the enum
    // silently defeats detection.
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "attribute_item" => {
                if let Ok(text) = s.utf8_text(source)
                    && text.contains("non_exhaustive")
                {
                    return true;
                }
            }
            "line_comment" | "block_comment" => {
                // Interleaved comment — keep walking.
            }
            _ => break,
        }
        sibling = s.prev_named_sibling();
    }
    false
}

/// Integer-primitive reprs that fix an enum's discriminant set as an FFI/ABI
/// contract, exactly like `#[repr(C)]`.
const C_ABI_INT_REPRS: &[&str] = &[
    "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize",
];

fn has_c_repr(item: tree_sitter::Node, source: &[u8]) -> bool {
    // Mirror `has_non_exhaustive`'s preceding-sibling walk through interleaved
    // comments; return true for `#[repr(C)]` or a C-style integer `#[repr(<int>)]`.
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "attribute_item" => {
                if attribute_is_c_repr(s, source) {
                    return true;
                }
            }
            "line_comment" | "block_comment" => {
                // Interleaved comment — keep walking.
            }
            _ => break,
        }
        sibling = s.prev_named_sibling();
    }
    false
}

/// True when an `attribute_item` is `#[repr(C)]` or `#[repr(<int-primitive>)]`.
/// Keys on the `repr` path *and* a `C`/integer-primitive argument, so unrelated
/// reprs such as `#[repr(align(...))]`/`#[repr(packed)]` are not matched.
fn attribute_is_c_repr(attribute_item: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(attribute) = attribute_item.named_child(0) else {
        return false;
    };
    if attribute.kind() != "attribute" {
        return false;
    }
    let is_repr = attribute
        .named_child(0)
        .filter(|path| path.kind() == "identifier")
        .and_then(|path| path.utf8_text(source).ok())
        .is_some_and(|text| text == "repr");
    if !is_repr {
        return false;
    }
    let Some(args) = attribute.child_by_field_name("arguments") else {
        return false;
    };
    let mut cursor = args.walk();
    args.named_children(&mut cursor).any(|arg| match arg.kind() {
        "identifier" => arg.utf8_text(source).is_ok_and(|text| text == "C"),
        "primitive_type" => arg
            .utf8_text(source)
            .is_ok_and(|text| C_ABI_INT_REPRS.contains(&text)),
        _ => false,
    })
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
    use std::fs;
    use tempfile::TempDir;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        // Use an absolute path with no Cargo.toml ancestor so is_internal_crate
        // does not accidentally pick up the comply project's own Cargo.toml.
        crate::rules::test_helpers::run_rule(&Check, source, "/nonexistent_cargo_project/src/t.rs")
    }

    #[test]
    fn flags_pub_enum_without_non_exhaustive() {
        assert_eq!(run_on("pub enum Status { Ok, Err }").len(), 1);
    }

    #[test]
    fn allows_pub_enum_with_non_exhaustive() {
        let source = "#[non_exhaustive]\npub enum Status { Ok, Err }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_private_enum() {
        assert!(run_on("enum Status { Ok, Err }").is_empty());
    }

    #[test]
    fn does_not_flag_repr_c_enum() {
        // #3971: a `#[repr(C)]` enum is a C-ABI type; its fixed integer-valued
        // variant set is the cross-language contract, so `#[non_exhaustive]`
        // (Rust-only, invisible to C) is the wrong fix.
        assert!(run_on("#[repr(C)]\npub enum E { A, B }").is_empty());
    }

    #[test]
    fn does_not_flag_repr_int_enum() {
        // #3971: a C-style integer repr fixes the discriminant set just like
        // `#[repr(C)]`.
        assert!(run_on("#[repr(u8)]\npub enum E { A, B }").is_empty());
    }

    #[test]
    fn does_not_flag_hyper_repr_c_ffi_enum() {
        // #3971: the hyper FFI return-code enum shape.
        let source = "#[repr(C)]\npub enum hyper_code { HYPERE_OK, HYPERE_ERROR }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_repr_c_enum_with_interleaved_comment_and_derive() {
        // #3971: comments and an unrelated `#[derive]` between the `#[repr(C)]`
        // attribute and the enum must not defeat the C-ABI detection.
        let source =
            "#[repr(C)]\n#[derive(Debug)]\n// tag values mirror the C header\npub enum E { A, B }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_pub_enum_with_unrelated_attribute() {
        // #3971: only `#[repr(C)]`/`#[repr(<int>)]` exempts — an unrelated
        // attribute such as `#[derive(Debug)]` leaves the enum flagged.
        assert_eq!(run_on("#[derive(Debug)]\npub enum E { A, B }").len(), 1);
    }

    #[test]
    fn does_not_flag_pub_crate_enum() {
        assert!(run_on("pub(crate) enum Status { Ok, Err }").is_empty());
    }

    #[test]
    fn does_not_flag_pub_super_enum() {
        assert!(run_on("pub(super) enum Status { Ok, Err }").is_empty());
    }

    #[test]
    fn treats_publish_false_crate_as_internal() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"internal\"\nversion = \"0.1.0\"\npublish = false\n",
        )
        .unwrap();

        assert!(is_internal_crate(&dir.path().join("src/lib.rs")));
    }

    #[test]
    fn treats_publish_false_crate_with_multiline_inline_table_as_internal() {
        // Regression for #1732: a `publish = false` crate whose Cargo.toml uses
        // a TOML 1.1 multiline inline table (which the `toml` 0.8 parser
        // rejects) must still be recognized as internal. The parse failure
        // fails open to "internal" rather than treating the crate as public.
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"biome_cli\"\nversion = \"0.0.0\"\npublish = false\n\n\
             [dependencies]\ntokio = {\n  workspace = true,\n  features  = [\"rt\", \"sync\"]\n}\n",
        )
        .unwrap();

        assert!(is_internal_crate(&dir.path().join("src/diagnostics.rs")));
    }

    #[test]
    fn flags_published_lib_crate_with_bare_pub_enum() {
        // Negative space for #1732: a genuinely published lib crate (parseable
        // manifest, no `publish = false`) with a bare `pub enum` is still
        // flagged — fail-open on parse failure must not suppress real APIs.
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"mylib\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::write(dir.path().join("src/lib.rs"), "").unwrap();

        assert!(!is_internal_crate(&dir.path().join("src/lib.rs")));
    }

    #[test]
    fn does_not_flag_pub_enum_in_binary_only_crate() {
        // #1469: a binary-only crate (no `[lib]`, no `src/lib.rs`) has no
        // external consumers, so a bare `pub enum` is not a SemVer concern.
        // Detection reuses the central `CargoManifest::is_binary_only` lever.
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"mytool\"\nversion = \"0.1.0\"\n[[bin]]\nname = \"mytool\"\npath = \"src/main.rs\"\n",
        )
        .unwrap();
        let src_path = dir.path().join("src/config.rs");
        let source = "pub enum Either<L, R> { Left(L), Right(R) }";
        fs::write(&src_path, source).unwrap();

        assert!(crate::rules::test_helpers::run_rule(&Check, source, &src_path).is_empty());
    }

    #[test]
    fn does_not_flag_pub_enum_in_cfg_test_module() {
        // #1469: a `pub enum` inside a `#[cfg(test)]` module is a test helper —
        // `#[non_exhaustive]` would force wildcard arms and weaken exhaustiveness
        // checking in tests.
        let source = "#[cfg(test)]\nmod tests {\n    pub enum FixtureProvider { Git, Hg }\n}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_pub_enum_under_tests_dir() {
        // #3846: a `pub enum` in a file under a `tests/` directory (integration
        // test, fixture) is not part of any crate's published API surface, so
        // `#[non_exhaustive]` is meaningless there. Exempt via the shared
        // `is_under_tests_dir` predicate, generalizing the inline `#[cfg(test)]`
        // exemption to the `tests/` directory.
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"mylib\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::write(dir.path().join("src/lib.rs"), "").unwrap();
        let fixture_path = dir.path().join("tests/syntax-tests/source/Rust/output.rs");
        let source = "pub enum OutputType { Pager, Stdout }";

        assert!(crate::rules::test_helpers::run_rule(&Check, source, &fixture_path).is_empty());
    }

    #[test]
    fn flags_pub_enum_in_published_lib_src() {
        // #3846: the same published crate still flags a bare `pub enum` in its
        // `src/` API surface — the `tests/` exemption must not leak to source.
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"mylib\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let src_path = dir.path().join("src/lib.rs");
        let source = "pub enum OutputType { Pager, Stdout }";
        fs::write(&src_path, source).unwrap();

        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, source, &src_path).len(),
            1
        );
    }

    #[test]
    fn treats_standalone_published_crate_as_external() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"mylib\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/lib.rs"), "").unwrap();

        assert!(!is_internal_crate(&dir.path().join("src/lib.rs")));
    }

    #[test]
    fn treats_workspace_member_without_publish_true_as_internal() {
        let dir = TempDir::new().unwrap();
        let member = dir.path().join("crates/internal");
        fs::create_dir_all(&member).unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/internal\"]\n",
        )
        .unwrap();
        fs::write(
            member.join("Cargo.toml"),
            "[package]\nname = \"internal\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        assert!(is_internal_crate(&member.join("src/lib.rs")));
    }

    #[test]
    fn treats_workspace_member_with_publish_true_as_public() {
        let dir = TempDir::new().unwrap();
        let member = dir.path().join("crates/public-api");
        fs::create_dir_all(member.join("src")).unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/public-api\"]\n",
        )
        .unwrap();
        fs::write(
            member.join("Cargo.toml"),
            "[package]\nname = \"public-api\"\nversion = \"0.1.0\"\npublish = true\n",
        )
        .unwrap();
        fs::write(member.join("src/lib.rs"), "").unwrap();

        assert!(!is_internal_crate(&member.join("src/lib.rs")));
    }

    #[test]
    fn treats_workspace_member_with_publish_registry_as_public() {
        let dir = TempDir::new().unwrap();
        let member = dir.path().join("crates/public-api");
        fs::create_dir_all(member.join("src")).unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/public-api\"]\n",
        )
        .unwrap();
        fs::write(
            member.join("Cargo.toml"),
            "[package]\nname = \"public-api\"\nversion = \"0.1.0\"\npublish = [\"crates-io\"]\n",
        )
        .unwrap();
        fs::write(member.join("src/lib.rs"), "").unwrap();

        assert!(!is_internal_crate(&member.join("src/lib.rs")));
    }
}
