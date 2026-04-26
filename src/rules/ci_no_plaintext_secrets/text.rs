//! Flag secret-like keys (password/token/api_key/secret) with literal values in a
//! workflow. `${{ secrets.* }}` references are allowed.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers::{pair_key_text, pair_scalar_value};

/// Key fragments that indicate a secret-bearing value. Matched case-insensitively
/// as a substring so `DB_PASSWORD`, `API_TOKEN`, `GITHUB_TOKEN`, `SECRET_KEY`
/// and friends all trigger.
const SECRET_KEY_FRAGMENTS: &[&str] = &[
    "password",
    "passwd",
    "token",
    "secret",
    "api_key",
    "apikey",
    "private_key",
];

fn key_looks_like_secret(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    SECRET_KEY_FRAGMENTS
        .iter()
        .any(|frag| lower.contains(frag))
}

fn is_secret_reference(value: &str) -> bool {
    // Allow `${{ secrets.X }}`, `${{ vars.X }}`, or any `${{ ... }}` expression —
    // these are resolved at runtime and don't commit a literal.
    value.contains("${{")
}

crate::ast_check! { on ["block_mapping_pair"] => |node, source, ctx, diagnostics|
    let Some(key) = pair_key_text(node, source) else { return; };
    if !key_looks_like_secret(&key) { return; }
    let Some(value) = pair_scalar_value(node, source) else { return; };
    let value = value.split('#').next().unwrap_or("").trim();
    // A blank value means the actual scalar lives on the next line
    // (block scalar). We conservatively skip — avoids multi-line parsing.
    if value.is_empty() { return; }
    if is_secret_reference(value) { return; }

    let key_node = node.named_child(0).unwrap_or(node);
    let pos = key_node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ci-no-plaintext-secrets".into(),
        message: format!(
            "`{key}` has a literal value — reference `${{{{ secrets.{} }}}}` instead.",
            key.to_ascii_uppercase()
        ),
        severity: Severity::Error,
        span: Some((key_node.byte_range().start, key_node.byte_range().len())),
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
    fn flags_literal_token() {
        let yaml = "\
on: push
jobs:
  build:
    env:
      GITHUB_TOKEN: ghp_abcdef1234567890
";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn flags_literal_api_key_in_with() {
        let yaml = "\
on: push
jobs:
  build:
    steps:
      - uses: some/action@v1
        with:
          api_key: sk_live_12345
";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_secret_expression() {
        let yaml = "\
on: push
jobs:
  build:
    env:
      GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn ignores_non_secret_keys() {
        let yaml = "on: push\njobs:\n  build:\n    env:\n      NODE_VERSION: 20";
        assert!(run(yaml).is_empty());
    }
}
