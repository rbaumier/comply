//! Flag `actions/cache` steps whose `key:` lacks `hashFiles(...)`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers::{find_pair, pair_key_text, pair_scalar_value, pair_value_node};

/// A `block_sequence_item` (a step) is the parent of the `uses:` pair's
/// mapping. Walk up twice to reach it so we can look for sibling `with:`.
fn step_mapping<'a>(uses_pair: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    uses_pair.parent()
}

crate::ast_check! { on ["block_mapping_pair"] => |node, source, ctx, diagnostics|
    if pair_key_text(node, source).as_deref() != Some("uses") { return; }
    let Some(value) = pair_scalar_value(node, source) else { return; };
    let action = value.split('@').next().unwrap_or("").trim();
    if action != "actions/cache" { return; }

    // The step mapping (`block_mapping`) holds every sibling pair of `uses:`.
    let Some(step_map) = step_mapping(node) else { return; };
    let with_pair = find_pair(step_map, source, "with");
    let key_pair = with_pair
        .and_then(|p| pair_value_node(p))
        .and_then(|v| {
            let with_map = crate::rules::yaml_k8s_helpers::as_mapping(v)?;
            find_pair(with_map, source, "key")
        });

    let Some(value_node) = node.named_child(1) else { return; };
    let pos = value_node.start_position();

    let Some(key_pair) = key_pair else {
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: 1,
            rule_id: "ci-cache-key-includes-lockfile".into(),
            message: "actions/cache step has no `key:` — add one that includes \
                      `${{ hashFiles('**/package-lock.json') }}`."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
        return;
    };

    let key_text = pair_scalar_value(key_pair, source).unwrap_or_default();
    if !key_text.contains("hashFiles") {
        let key_value = key_pair.named_child(1).unwrap_or(key_pair);
        let kpos = key_value.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: kpos.row + 1,
            column: 1,
            rule_id: "ci-cache-key-includes-lockfile".into(),
            message: "actions/cache `key:` omits `hashFiles(...)` — include \
                      `${{ hashFiles('**/package-lock.json') }}` so the cache \
                      invalidates when dependencies change."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
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
    fn flags_static_key() {
        let yaml = "\
on: push
jobs:
  build:
    steps:
      - uses: actions/cache@v4
        with:
          path: ~/.npm
          key: npm-cache
";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_hashfiles_key() {
        let yaml = "\
on: push
jobs:
  build:
    steps:
      - uses: actions/cache@v4
        with:
          path: ~/.npm
          key: npm-${{ hashFiles('**/package-lock.json') }}
";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn ignores_other_uses() {
        let yaml = "on: push\njobs:\n  build:\n    steps:\n    - uses: actions/checkout@v4";
        assert!(run(yaml).is_empty());
    }
}
