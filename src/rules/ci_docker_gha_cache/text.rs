//! Flag `docker/build-push-action` steps without `cache-from:` in their `with:` block.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers::{as_mapping, find_pair, pair_key_text, pair_scalar_value, pair_value_node};

crate::ast_check! { on ["block_mapping_pair"] => |node, source, ctx, diagnostics|
    if pair_key_text(node, source).as_deref() != Some("uses") { return; }
    let Some(value) = pair_scalar_value(node, source) else { return; };
    let action = value.split('@').next().unwrap_or("").trim();
    if action != "docker/build-push-action" { return; }

    // Look for `with.cache-from` within the same step mapping.
    let Some(step_map) = node.parent() else { return; };
    let with_map = find_pair(step_map, source, "with")
        .and_then(pair_value_node)
        .and_then(as_mapping);
    let has_cache_from = with_map
        .map(|m| find_pair(m, source, "cache-from").is_some())
        .unwrap_or(false);

    if has_cache_from { return; }

    let Some(value_node) = node.named_child(1) else { return; };
    let pos = value_node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: 1,
        rule_id: "ci-docker-gha-cache".into(),
        message: "docker/build-push-action has no `cache-from:` — add \
                  `cache-from: type=gha` and `cache-to: type=gha,mode=max` \
                  to reuse layer cache across runs."
            .into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use crate::rules::test_helpers::run_yaml_with_path;

    fn run(source: &str) -> Vec<Diagnostic> {
        run_yaml_with_path(source, &Check, ".github/workflows/docker.yml")
    }

    #[test]
    fn flags_missing_cache() {
        let yaml = "\
on: push
jobs:
  build:
    steps:
      - uses: docker/build-push-action@v5
        with:
          push: true
          tags: app:latest
";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_with_gha_cache() {
        let yaml = "\
on: push
jobs:
  build:
    steps:
      - uses: docker/build-push-action@v5
        with:
          push: true
          cache-from: type=gha
          cache-to: type=gha,mode=max
";
        assert!(run(yaml).is_empty());
    }
}
