//! rust-lib-without-missing-docs backend.
//!
//! Runs once per file, on the `source_file` node, and only for a file named
//! `lib.rs` — the crate root is the single place a crate-wide lint level can be
//! written, so it is also the only place the rule can report the omission. The
//! `[lib] path = "…"` escape hatch (a crate root under another name) is not
//! resolved: the parsed manifest does not carry that field, and the convention
//! is near-universal.
//!
//! The file is flagged when all of the following hold:
//!
//! - it exposes at least one bare-`pub` top-level item — a `fn`, `struct`,
//!   `enum`, `union`, `trait`, `mod`, `type`, `const`, `static`, or a `pub use`
//!   re-export. A crate root made only of private `mod` declarations has no
//!   public surface to document, so it is left alone; `pub(crate)` is not public
//!   API either and does not count;
//! - no crate-level inner attribute sets a level for `missing_docs`
//!   (`#![deny(missing_docs)]`, `#![warn(…)]`, `#![forbid(…)]`, a
//!   `#![cfg_attr(docsrs, deny(missing_docs))]`, or a list that names it among
//!   other lints). `#![allow(missing_docs)]` / `#![expect(missing_docs)]` count
//!   too: writing the lint name at all is a deliberate decision about it, which
//!   is what the rule asks for — silence is the thing it reports;
//! - the crate's `Cargo.toml` does not declare it either, through
//!   `[lints.rust] missing_docs = …` or through `[lints] workspace = true` with
//!   `missing_docs` under the workspace root's `[workspace.lints.rust]`.
//!
//! An unpublished crate (`publish = false`) is exempt: it has no external
//! consumer reading its rustdoc, so the lint buys nothing. So is everything
//! [`is_library_code`] rejects — most relevantly the `lib.rs` of a crate that
//! also ships a binary, where the `[lib]` usually exists only to share code with
//! the crate's own `main.rs` and integration tests.
//!
//! Manifest resolution reads and parses the `Cargo.toml` directly rather than
//! going through the shared `CargoManifest`, which does not carry the `[lints]`
//! table. Both reads happen at most once per crate root, since the rule visits
//! one node in one file per crate. Whenever a manifest or a workspace root can
//! be located but not read or parsed, the rule stays silent rather than report
//! on a guess.

use std::path::Path;

use tree_sitter::Node;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{is_library_code, is_pub};

/// Top-level item kinds that can carry a `pub` and thereby put something on the
/// crate's public surface.
const PUBLIC_ITEM_KINDS: &[&str] = &[
    "function_item",
    "struct_item",
    "enum_item",
    "union_item",
    "trait_item",
    "mod_item",
    "type_item",
    "const_item",
    "static_item",
    "use_declaration",
];

/// Attribute paths that set (or deliberately unset) a lint level. A
/// `missing_docs` mention under any of them is an explicit decision about the
/// lint; a mention anywhere else (a doc string quoting the name) is not.
const LINT_LEVEL_ATTRIBUTES: &[&str] = &["deny", "warn", "forbid", "allow", "expect", "cfg_attr"];

/// The rustc lint name, spelled as `Cargo.toml` and the attributes spell it.
const MISSING_DOCS: &str = "missing_docs";

crate::ast_check! { on ["source_file"] prefilter = ["pub"] => |node, source, ctx, diagnostics|
    if ctx.path.file_name().and_then(|name| name.to_str()) != Some("lib.rs") { return; }
    if declares_missing_docs_level(node, source) { return; }
    if !exposes_public_item(node, source) { return; }
    if !is_library_code(node, source, ctx) { return; }
    if manifest_settles_missing_docs(ctx) { return; }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: 1,
        column: 1,
        rule_id: super::META.id.into(),
        message: "this crate root exposes public API but never sets a level for `missing_docs`, \
                  which is `allow` by default — undocumented `pub` items ship without a warning. \
                  Add `#![deny(missing_docs)]` here, or `missing_docs = \"warn\"` under \
                  `[lints.rust]` in `Cargo.toml`."
            .into(),
        severity: Severity::Error,
        span: None,
    });
}

/// True when a crate-level inner attribute (`#![…]`) names `missing_docs` under
/// a lint-level path. Only direct children of the file are read: an attribute
/// inside a `mod` block sets the level for that module, not for the crate.
fn declares_missing_docs_level(source_file: Node, source: &[u8]) -> bool {
    let mut cursor = source_file.walk();
    source_file.children(&mut cursor).any(|child| {
        child.kind() == "inner_attribute_item" && attribute_sets_missing_docs(child, source)
    })
}

/// True when an attribute node's text names `missing_docs` and its path is a
/// lint-level one. The path test is what keeps a doc attribute that merely
/// quotes the lint name (`#![doc = "… missing_docs …"]`) from counting.
fn attribute_sets_missing_docs(attribute_item: Node, source: &[u8]) -> bool {
    let Ok(text) = attribute_item.utf8_text(source) else {
        return false;
    };
    if !text.contains(MISSING_DOCS) {
        return false;
    }
    let Some(attribute) = attribute_item.named_child(0) else {
        return false;
    };
    let Some(path) = attribute
        .named_child(0)
        .and_then(|p| p.utf8_text(source).ok())
    else {
        return false;
    };
    let last_segment = path.rsplit("::").next().unwrap_or(path);
    LINT_LEVEL_ATTRIBUTES.contains(&last_segment)
}

/// True when a top-level item of the file is declared bare `pub`.
fn exposes_public_item(source_file: Node, source: &[u8]) -> bool {
    let mut cursor = source_file.walk();
    source_file
        .children(&mut cursor)
        .any(|child| PUBLIC_ITEM_KINDS.contains(&child.kind()) && is_pub(child, source))
}

/// True when the crate's `Cargo.toml` already answers for `missing_docs` —
/// either by declaring the lint (directly or through the workspace) or by
/// opting out of publication, which removes the consumer the docs are for.
///
/// A manifest that cannot be located answers `false` so the rule keeps
/// flagging; one that can be located but not read or parsed answers `true`, so
/// a manifest the rule failed to understand never turns into a diagnostic.
fn manifest_settles_missing_docs(ctx: &crate::rules::backend::CheckCtx) -> bool {
    let Some(manifest) = ctx.project.nearest_cargo_manifest(ctx.path) else {
        return false;
    };
    let crate_dir = manifest.manifest_dir();
    let Some(value) = read_manifest(crate_dir) else {
        return true;
    };
    if is_unpublished(&value) || lints_table_names_missing_docs(&value) {
        return true;
    }
    inherits_workspace_lints(&value) && workspace_root_names_missing_docs(crate_dir)
}

/// Parse the `Cargo.toml` sitting in `dir`, or `None` when it cannot be read or
/// is not valid TOML.
fn read_manifest(dir: &Path) -> Option<toml::Value> {
    std::fs::read_to_string(dir.join("Cargo.toml"))
        .ok()?
        .parse::<toml::Value>()
        .ok()
}

/// True when `[package] publish` opts the crate out of publication — either
/// `publish = false` or the empty-registry-list form `publish = []`.
fn is_unpublished(manifest: &toml::Value) -> bool {
    let Some(publish) = manifest.get("package").and_then(|p| p.get("publish")) else {
        return false;
    };
    match publish {
        toml::Value::Boolean(allowed) => !allowed,
        toml::Value::Array(registries) => registries.is_empty(),
        _ => false,
    }
}

/// True when `[lints.rust]` carries a `missing_docs` key. The `-` spelling
/// rustc accepts on the command line is matched too, so a manifest written that
/// way is not flagged for a spelling.
fn lints_table_names_missing_docs(manifest: &toml::Value) -> bool {
    let Some(rust_lints) = manifest.get("lints").and_then(|lints| lints.get("rust")) else {
        return false;
    };
    rust_lints.get(MISSING_DOCS).is_some() || rust_lints.get("missing-docs").is_some()
}

/// True when the crate defers its lint configuration to the workspace
/// (`[lints] workspace = true`).
fn inherits_workspace_lints(manifest: &toml::Value) -> bool {
    manifest
        .get("lints")
        .and_then(|lints| lints.get("workspace"))
        .and_then(toml::Value::as_bool)
        .unwrap_or(false)
}

/// True when the workspace root above `crate_dir` declares `missing_docs` under
/// `[workspace.lints.rust]`.
///
/// The walk starts at `crate_dir` itself, since a single-crate workspace carries
/// `[workspace]` and `[package]` in the same file. A root that cannot be found,
/// read, or parsed answers `true`: the crate said its lints live in the
/// workspace, and failing to read that workspace is no reason to accuse it.
fn workspace_root_names_missing_docs(crate_dir: &Path) -> bool {
    let Some(root_manifest) = workspace_root_manifest(crate_dir) else {
        return true;
    };
    let Some(rust_lints) = root_manifest
        .get("workspace")
        .and_then(|workspace| workspace.get("lints"))
        .and_then(|lints| lints.get("rust"))
    else {
        return false;
    };
    rust_lints.get(MISSING_DOCS).is_some() || rust_lints.get("missing-docs").is_some()
}

/// The parsed manifest of the nearest ancestor (or `crate_dir` itself) whose
/// `Cargo.toml` carries a `[workspace]` table, or `None` when there is none or
/// it cannot be read.
fn workspace_root_manifest(crate_dir: &Path) -> Option<toml::Value> {
    let mut dir: Option<&Path> = Some(crate_dir);
    while let Some(candidate) = dir {
        if let Some(manifest) = read_manifest(candidate)
            && manifest.get("workspace").is_some()
        {
            return Some(manifest);
        }
        dir = candidate.parent();
    }
    None
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

    const LIB_CARGO_TOML: &str = "[package]\nname = \"mylib\"\nversion = \"0.1.0\"\n\
        edition = \"2021\"\n";

    const BIN_CARGO_TOML: &str = "[package]\nname = \"mytool\"\nversion = \"0.1.0\"\n\
        edition = \"2021\"\n\n[[bin]]\nname = \"mytool\"\npath = \"src/main.rs\"\n";

    /// Run on `rel_path` inside a temp crate carrying `cargo_toml`, so both the
    /// library classification and the `[lints]` lookup resolve against a
    /// controlled manifest instead of comply's own.
    fn run_in_crate(cargo_toml: &str, rel_path: &str, source: &str) -> Vec<Diagnostic> {
        run_in_workspace(&[("Cargo.toml", cargo_toml)], rel_path, source)
    }

    /// Run on `rel_path` inside a temp directory pre-populated with `files`
    /// (each a `(path relative to the temp root, contents)` pair), so a
    /// workspace root and a member crate can both be laid out.
    fn run_in_workspace(files: &[(&str, &str)], rel_path: &str, source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        for (rel, contents) in files {
            let path = dir.path().join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
        let src_path = dir.path().join(rel_path);
        fs::create_dir_all(src_path.parent().unwrap()).unwrap();
        fs::write(&src_path, source).unwrap();
        crate::rules::test_helpers::run_rule(&Check, source, &src_path)
    }

    fn run_in_lib(source: &str) -> Vec<Diagnostic> {
        run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source)
    }

    #[test]
    fn flags_lib_root_with_public_items_and_no_lint_level() {
        let source = "pub mod parser;\npub struct Config;\n";
        assert_eq!(run_in_lib(source).len(), 1);
    }

    #[test]
    fn flags_lib_root_exposing_only_a_pub_use() {
        let source = "mod inner;\npub use inner::Config;\n";
        assert_eq!(run_in_lib(source).len(), 1);
    }

    /// The diagnostic points at the crate root itself, not at any one item.
    #[test]
    fn reports_on_the_first_line() {
        let diagnostics = run_in_lib("pub fn parse() {}\n");
        assert_eq!((diagnostics[0].line, diagnostics[0].column), (1, 1));
    }

    #[test]
    fn allows_lib_root_with_deny_missing_docs() {
        let source = "#![deny(missing_docs)]\n//! Docs.\npub struct Config;\n";
        assert!(run_in_lib(source).is_empty());
    }

    #[test]
    fn allows_lib_root_with_warn_missing_docs() {
        let source = "#![warn(missing_docs)]\npub struct Config;\n";
        assert!(run_in_lib(source).is_empty());
    }

    #[test]
    fn allows_lib_root_with_missing_docs_in_a_lint_list() {
        let source =
            "#![deny(rustdoc::broken_intra_doc_links, missing_docs)]\npub struct Config;\n";
        assert!(run_in_lib(source).is_empty());
    }

    /// `cfg_attr` is how a crate scopes the lint to its docs.rs build.
    #[test]
    fn allows_lib_root_with_cfg_attr_deny_missing_docs() {
        let source = "#![cfg_attr(docsrs, deny(missing_docs))]\npub struct Config;\n";
        assert!(run_in_lib(source).is_empty());
    }

    /// An explicit `allow` is still a decision about the lint.
    #[test]
    fn allows_lib_root_with_allow_missing_docs() {
        let source = "#![allow(missing_docs)]\npub struct Config;\n";
        assert!(run_in_lib(source).is_empty());
    }

    /// A doc comment that merely quotes the lint name settles nothing.
    #[test]
    fn flags_lib_root_that_only_mentions_missing_docs_in_a_doc_attribute() {
        let source = "#![doc = \"we should turn on missing_docs one day\"]\npub struct Config;\n";
        assert_eq!(run_in_lib(source).len(), 1);
    }

    /// The level must be set at the crate root: a `mod`-scoped one leaves the
    /// rest of the public surface uncovered.
    #[test]
    fn flags_lib_root_with_missing_docs_scoped_to_a_module() {
        let source = "pub mod parser { #![deny(missing_docs)] }\npub struct Config;\n";
        assert_eq!(run_in_lib(source).len(), 1);
    }

    #[test]
    fn allows_lib_root_with_no_public_item() {
        let source = "mod inner;\nuse inner::Config;\npub(crate) fn helper() {}\n";
        assert!(run_in_lib(source).is_empty());
    }

    #[test]
    fn allows_lints_rust_missing_docs_in_the_manifest() {
        let cargo_toml = "[package]\nname = \"mylib\"\nversion = \"0.1.0\"\n\
            edition = \"2021\"\n\n[lints.rust]\nmissing_docs = \"warn\"\n";
        assert!(run_in_crate(cargo_toml, "src/lib.rs", "pub struct Config;\n").is_empty());
    }

    #[test]
    fn allows_unpublished_crate() {
        let cargo_toml = "[package]\nname = \"mylib\"\nversion = \"0.1.0\"\n\
            edition = \"2021\"\npublish = false\n";
        assert!(run_in_crate(cargo_toml, "src/lib.rs", "pub struct Config;\n").is_empty());
    }

    #[test]
    fn allows_workspace_lints_declaring_missing_docs() {
        let root = "[workspace]\nmembers = [\"crates/mylib\"]\n\n\
            [workspace.lints.rust]\nmissing_docs = \"warn\"\n";
        let member = "[package]\nname = \"mylib\"\nversion = \"0.1.0\"\n\
            edition = \"2021\"\n\n[lints]\nworkspace = true\n";
        let files = [("Cargo.toml", root), ("crates/mylib/Cargo.toml", member)];
        let diagnostics =
            run_in_workspace(&files, "crates/mylib/src/lib.rs", "pub struct Config;\n");
        assert!(diagnostics.is_empty());
    }

    /// Inheriting the workspace lints does not settle the question when the
    /// workspace itself never mentions `missing_docs`.
    #[test]
    fn flags_workspace_lints_without_missing_docs() {
        let root = "[workspace]\nmembers = [\"crates/mylib\"]\n\n\
            [workspace.lints.clippy]\nunwrap_used = \"warn\"\n";
        let member = "[package]\nname = \"mylib\"\nversion = \"0.1.0\"\n\
            edition = \"2021\"\n\n[lints]\nworkspace = true\n";
        let files = [("Cargo.toml", root), ("crates/mylib/Cargo.toml", member)];
        let diagnostics =
            run_in_workspace(&files, "crates/mylib/src/lib.rs", "pub struct Config;\n");
        assert_eq!(diagnostics.len(), 1);
    }

    /// A `[lib]` that exists to share code with the crate's own binary is not a
    /// published library surface.
    #[test]
    fn allows_lib_root_of_a_crate_that_ships_a_binary() {
        let source = "pub mod parser;\npub struct Config;\n";
        assert!(run_in_crate(BIN_CARGO_TOML, "src/lib.rs", source).is_empty());
    }

    /// The rule is scoped to the crate root; an ordinary module is not where a
    /// crate-wide lint level can be written.
    #[test]
    fn allows_ordinary_module_file() {
        let source = "pub struct Config;\n";
        assert!(run_in_crate(LIB_CARGO_TOML, "src/parser.rs", source).is_empty());
    }
}
