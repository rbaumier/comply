//! dockerfile-pin-exact-version tree-sitter backend.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["from_instruction"] => |node, source, ctx, diagnostics|
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
                image_tag_text = std::str::from_utf8(&source[child.byte_range()]).ok();
            }
            "image_digest" => has_image_digest = true,
            _ => {}
        }
    }
    if image_name_text == Some("scratch") { return; }
    if has_image_digest { return; }
    let Some(tag) = image_tag_text.and_then(|t| t.strip_prefix(':')) else {
        return; // No tag — handled by dockerfile-no-latest-tag.
    };
    if tag == "latest" { return; }
    if is_bare_major(tag) {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message: "FROM tag pins only a major version; pin a precise version such as `22.12-alpine3.20`.".into(),
            severity: Severity::Warning,
            span: Some((node.byte_range().start, node.byte_range().len())),
        });
    }
}

/// A tag is "bare major" when it contains only digits, e.g. `22` or `3`.
fn is_bare_major(tag: &str) -> bool {
    !tag.is_empty() && tag.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_dockerfile(s, &Check)
    }

    #[test]
    fn flags_bare_major_tag() {
        assert_eq!(run("FROM node:22\n").len(), 1);
    }

    #[test]
    fn allows_precise_tag() {
        assert!(run("FROM node:22.12-alpine3.20\n").is_empty());
    }

    #[test]
    fn ignores_latest() {
        assert!(run("FROM node:latest\n").is_empty());
    }
}
