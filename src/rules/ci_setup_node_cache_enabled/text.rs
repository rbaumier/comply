//! Flag `uses: actions/setup-node@...` steps whose `with:` block omits a `cache:` key.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers::{
    as_mapping, find_pair, pair_key_text, pair_scalar_value, pair_value_node,
};

crate::ast_check! { on ["block_mapping_pair"] => |node, source, ctx, diagnostics|
    if pair_key_text(node, source).as_deref() != Some("uses") { return; }
    let Some(value) = pair_scalar_value(node, source) else { return; };
    let action = value.split('@').next().unwrap_or("").trim();
    if action != "actions/setup-node" { return; }

    // Sibling `with:` inside the step mapping.
    let Some(step_map) = node.parent() else { return; };
    let has_cache = find_pair(step_map, source, "with")
        .and_then(pair_value_node)
        .and_then(as_mapping)
        .map(|m| find_pair(m, source, "cache").is_some())
        .unwrap_or(false);
    if has_cache { return; }

    let Some(value_node) = node.named_child(1) else { return; };
    let pos = value_node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: 1,
        rule_id: "ci-setup-node-cache-enabled".into(),
        message: "actions/setup-node is used without `cache:` — add `cache: 'npm'` \
                  (or pnpm/yarn) to reuse the dependency cache across runs."
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
        run_yaml_with_path(source, &Check, ".github/workflows/ci.yml")
    }

    #[test]
    fn flags_missing_cache() {
        let yaml = "\
on: push
jobs:
  build:
    steps:
      - uses: actions/setup-node@v4
        with:
          node-version: 20
";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn flags_no_with_block() {
        let yaml = "\
on: push
jobs:
  build:
    steps:
      - uses: actions/setup-node@v4
";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_with_cache() {
        let yaml = "\
on: push
jobs:
  build:
    steps:
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: 'npm'
";
        assert!(run(yaml).is_empty());
    }
}
