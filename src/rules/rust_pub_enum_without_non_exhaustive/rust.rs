//! rust-pub-enum-without-non-exhaustive backend.
//!
//! Walks `enum_item` nodes with `pub` visibility and scans the
//! preceding `attribute_item` siblings for `#[non_exhaustive]`. If
//! absent, flag.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
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
    let Ok(content) = std::fs::read_to_string(&manifest) else {
        return false;
    };
    let Ok(value) = content.parse::<toml::Value>() else {
        return false;
    };

    if !value.get("package").is_some_and(toml::Value::is_table) {
        return true;
    }
    if publish_is_disabled(&value) {
        return true;
    }

    // Binary-only crate: no [lib] section in Cargo.toml AND no src/lib.rs
    // (Cargo auto-detects src/lib.rs as the default library target when [lib] is absent).
    let has_lib_section = value.get("lib").is_some_and(toml::Value::is_table);
    let has_lib_rs = manifest
        .parent()
        .map_or(false, |root| root.join("src/lib.rs").is_file());
    if !has_lib_section && !has_lib_rs {
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
    fn treats_binary_only_crate_as_internal() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"mybin\"\nversion = \"0.1.0\"\n[[bin]]\nname = \"mybin\"\npath = \"src/main.rs\"\n",
        )
        .unwrap();

        assert!(is_internal_crate(&dir.path().join("src/main.rs")));
    }

    #[test]
    fn treats_lib_crate_without_explicit_lib_section_as_external() {
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
