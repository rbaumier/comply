//! k8s-no-latest-image-tag tree-sitter backend (YAML AST).
//!
//! Flags every container `image:` value that uses `:latest` or omits a tag.
//! Digest-pinned images (`image@sha256:...`) are accepted.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) {
        return;
    }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    let Some(pod_spec) = y::pod_spec_mapping(node, source, &kind) else { return; };
    for container in y::containers_of_pod_spec(pod_spec, source, true) {
        let Some(image_pair) = y::find_pair(container, source, "image") else { continue; };
        let Some(image) = y::pair_scalar_value(image_pair, source) else { continue; };
        if image.is_empty() || image.contains('@') {
            continue;
        }
        let tag = last_tag(&image);
        if tag == Some("latest") || tag.is_none() {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &image_pair,
                super::META.id,
                "Container image uses `:latest` or omits a tag; pin an explicit version or digest.".into(),
                Severity::Warning,
            ));
        }
    }
}

/// Returns the tag portion of an image reference, or `None` if no tag.
/// Handles registry ports correctly (`registry:5000/image:tag` → `tag`).
fn last_tag(image: &str) -> Option<&str> {
    let after_slash = image.rsplit('/').next().unwrap_or(image);
    let (_, tag) = after_slash.split_once(':')?;
    Some(tag)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_yaml;

    fn run(source: &str) -> Vec<Diagnostic> {
        run_yaml(source, &Check)
    }

    #[test]
    fn flags_latest_tag() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  template:\n    spec:\n      containers:\n      - name: app\n        image: nginx:latest";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn flags_missing_tag() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  template:\n    spec:\n      containers:\n      - name: app\n        image: nginx";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_pinned_tag() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  template:\n    spec:\n      containers:\n      - name: app\n        image: nginx:1.25.3";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn allows_digest_pin() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  template:\n    spec:\n      containers:\n      - name: app\n        image: nginx@sha256:abcd";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn ignores_non_k8s_yaml() {
        let yaml = "image: nginx:latest";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn handles_registry_port() {
        let yaml = "apiVersion: v1\nkind: Pod\nspec:\n  containers:\n  - image: registry.local:5000/app:1.0";
        assert!(run(yaml).is_empty());
    }
}
