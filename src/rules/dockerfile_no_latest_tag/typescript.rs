//! dockerfile-no-latest-tag tree-sitter backend.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "from_instruction" { return; }
    // Find image_spec child.
    let mut image_spec = None;
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        if child.kind() == "image_spec" {
            image_spec = Some(child);
            break;
        }
    }
    let Some(image_spec) = image_spec else { return; };

    // Inspect image_spec children.
    let mut has_image_tag = false;
    let mut has_image_digest = false;
    let mut image_name_text: Option<&str> = None;
    let mut image_tag_text: Option<&str> = None;
    for i in 0..image_spec.child_count() {
        let child = image_spec.child(i).unwrap();
        match child.kind() {
            "image_name" => {
                image_name_text = std::str::from_utf8(&source[child.byte_range()]).ok();
            }
            "image_tag" => {
                has_image_tag = true;
                image_tag_text = std::str::from_utf8(&source[child.byte_range()]).ok();
            }
            "image_digest" => {
                has_image_digest = true;
            }
            _ => {}
        }
    }
    // `FROM scratch` is allowed.
    if image_name_text == Some("scratch") { return; }
    // Digest-pinned images are allowed even without a tag.
    if has_image_digest { return; }

    // Strip the leading `:` from the tag text.
    let tag_after_colon = image_tag_text.and_then(|t| t.strip_prefix(':'));
    let is_latest = tag_after_colon == Some("latest");
    if !has_image_tag || is_latest {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message: "FROM image uses `:latest` or no tag; pin an explicit version.".into(),
            severity: Severity::Warning,
            span: Some((node.byte_range().start, node.byte_range().len())),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_dockerfile(s, &Check)
    }

    #[test]
    fn flags_latest_tag() {
        assert_eq!(run("FROM node:latest\n").len(), 1);
    }

    #[test]
    fn flags_missing_tag() {
        assert_eq!(run("FROM node\n").len(), 1);
    }

    #[test]
    fn allows_pinned_version() {
        assert!(run("FROM node:22.12-alpine3.20\n").is_empty());
    }

    #[test]
    fn allows_scratch() {
        assert!(run("FROM scratch\n").is_empty());
    }

    #[test]
    fn allows_digest_pin() {
        assert!(run("FROM node@sha256:abc123\n").is_empty());
    }
}
