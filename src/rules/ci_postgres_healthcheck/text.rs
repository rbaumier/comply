//! Flag `services:` blocks that declare a postgres service but omit a pg_isready
//! health-check in the `options:` line.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers::{find_pair, pair_key_text, pair_scalar_value};

fn is_postgres_image(value: &str) -> bool {
    let name = value.trim().split(':').next().unwrap_or("").trim();
    // Accept "postgres", "postgres:<tag>", "docker.io/library/postgres", or a
    // registry-prefixed form ending in `/postgres`.
    name == "postgres" || name.ends_with("/postgres")
}

/// Raw text of a pair's value node — works for flow scalars and block scalars
/// alike. `options:` is commonly written as a folded block scalar (`>-`),
/// which `pair_scalar_value` skips because it only handles `flow_node`.
fn pair_value_text<'a>(
    pair: tree_sitter::Node<'a>,
    source: &'a [u8],
) -> Option<&'a str> {
    pair.named_child(1)?.utf8_text(source).ok()
}

crate::ast_check! { on ["block_mapping_pair"] => |node, source, ctx, diagnostics|
    if pair_key_text(node, source).as_deref() != Some("image") { return; }
    let Some(value) = pair_scalar_value(node, source) else { return; };
    if !is_postgres_image(&value) { return; }

    // The service mapping is the parent of this `image:` pair. Walk its
    // sibling pairs (image / env / options / …) for `options:` with
    // `--health-cmd pg_isready`.
    let Some(service_map) = node.parent() else { return; };
    let options = find_pair(service_map, source, "options")
        .and_then(|p| pair_value_text(p, source));
    let has_health = options
        .map(|o| o.contains("--health-cmd") && o.contains("pg_isready"))
        .unwrap_or(false);
    if has_health { return; }

    let Some(value_node) = node.named_child(1) else { return; };
    let pos = value_node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: 1,
        rule_id: "ci-postgres-healthcheck".into(),
        message: "postgres service is missing `--health-cmd pg_isready` \
                  — downstream steps can race db startup."
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
    fn flags_missing_healthcheck() {
        let yaml = "\
on: push
jobs:
  test:
    services:
      postgres:
        image: postgres:15
        env:
          POSTGRES_PASSWORD: x
";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_with_pg_isready() {
        let yaml = "\
on: push
jobs:
  test:
    services:
      postgres:
        image: postgres:15
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn ignores_non_postgres_service() {
        let yaml = "\
on: push
jobs:
  test:
    services:
      redis:
        image: redis:7
";
        assert!(run(yaml).is_empty());
    }
}
