//! Flag secret-like keys (password/token/api_key/secret) with literal values in a
//! workflow. `${{ secrets.* }}` references are allowed.
//! GitLab `$VAR` references are allowed.
//! Boolean and short numeric flags are allowed too.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers::{pair_key_text, pair_scalar_value};
use std::path::Path;

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
    SECRET_KEY_FRAGMENTS.iter().any(|frag| lower.contains(frag))
}

fn is_secret_reference(value: &str) -> bool {
    // Allow `${{ secrets.X }}`, `${{ vars.X }}`, or any `${{ ... }}` expression —
    // these are resolved at runtime and don't commit a literal.
    value.contains("${{") || is_variable_reference(value)
}

/// A whole value `$NAME` or `${NAME}`: a GitLab CI/CD variable.
fn is_variable_reference(value: &str) -> bool {
    let Some(rest) = value.strip_prefix('$') else { return false; };
    let name = rest
        .strip_prefix('{')
        .and_then(|braced| braced.strip_suffix('}'))
        .unwrap_or(rest);
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Feature flags and short numbers carry no credential.
/// Example: `REQUIRE_CURRENT_PASSWORD: "1"`.
fn is_flag_value(value: &str) -> bool {
    const BOOLEANS: &[&str] = &["true", "false", "yes", "no", "on", "off"];
    BOOLEANS.iter().any(|b| value.eq_ignore_ascii_case(b))
        || (value.len() <= 5 && value.bytes().all(|b| b.is_ascii_digit()))
}

/// GitLab CI: `.gitlab-ci.yml` and its local includes `*.gitlab-ci.yml`.
fn is_gitlab_ci(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|n| n.ends_with(".gitlab-ci.yml") || n.ends_with(".gitlab-ci.yaml"))
}

crate::ast_check! { on ["block_mapping_pair"] => |node, source, ctx, diagnostics|
    let Some(key) = pair_key_text(node, source) else { return; };
    if !key_looks_like_secret(&key) { return; }
    let Some(value) = pair_scalar_value(node, source) else { return; };
    let value = value.split('#').next().unwrap_or("").trim();
    // A blank value means the actual scalar lives on the next line
    // (block scalar). We conservatively skip — avoids multi-line parsing.
    if value.is_empty() { return; }
    if is_secret_reference(value) || is_flag_value(value) { return; }

    let key_node = node.named_child(0).unwrap_or(node);
    let pos = key_node.start_position();
    let name = key.to_ascii_uppercase();
    let message = if is_gitlab_ci(ctx.path) {
        format!("`{key}` has a literal value — move it to a masked CI/CD variable and reference `${name}` instead.")
    } else {
        format!("`{key}` has a literal value — reference `${{{{ secrets.{name} }}}}` instead.")
    };
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ci-no-plaintext-secrets".into(),
        message,
        severity: Severity::Error,
        span: Some((key_node.byte_range().start, key_node.byte_range().len())),
    });
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
    use crate::diagnostic::Diagnostic;
    
    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, ".github/workflows/ci.yml")
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

    fn run_at(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
    }

    #[test]
    fn allows_gitlab_variable_reference() {
        let yaml = "\
deploy:
  variables:
    CLOUDFLARE_API_TOKEN: $FRANCE_NUAGE_CF_TOKEN
    DEPLOY_TOKEN: ${DEPLOY_TOKEN_MASKED}
    DB_PASSWORD: \"$CI_DB_PASSWORD\"
";
        assert!(run_at(yaml, ".gitlab-ci.yml").is_empty());
    }

    #[test]
    fn allows_boolean_and_short_numeric_flag_values() {
        let yaml = "\
test:
  variables:
    GOTRUE_SECURITY_UPDATE_PASSWORD_REQUIRE_CURRENT_PASSWORD: \"true\"
    REQUIRE_GOTRUE_CURRENT_PASSWORD: \"1\"
    PASSWORD_RESET_ENABLED: false
    SECRET_SCANNING: \"0\"
    TOKEN_TTL_SECONDS: 86400
";
        assert!(run_at(yaml, ".gitlab-ci.yml").is_empty());
        assert!(run_at(yaml, "ci/test.gitlab-ci.yml").is_empty());
    }

    #[test]
    fn still_flags_literal_secrets_in_gitlab_ci() {
        let yaml = "\
test:
  services:
    - name: postgres:17
      variables:
        POSTGRES_PASSWORD: ci-throwaway-pw
        GOTRUE_JWT_SECRET: ci-throwaway-jwt-secret-32-characters-min
  variables:
    DB_PASSWORD: pa$$w0rd
    API_TOKEN: 123456
";
        assert_eq!(run_at(yaml, ".gitlab-ci.yml").len(), 4);
    }

    #[test]
    fn gitlab_message_suggests_masked_variable() {
        let yaml = "test:\n  variables:\n    POSTGRES_PASSWORD: ci-throwaway-pw\n";
        let diags = run_at(yaml, ".gitlab-ci.yml");
        assert_eq!(diags.len(), 1);
        let message = &diags[0].message;
        assert!(message.contains("masked CI/CD variable"), "{message}");
        assert!(message.contains("`$POSTGRES_PASSWORD`"), "{message}");
        assert!(!message.contains("${{"), "{message}");
    }

    #[test]
    fn github_keeps_expressions_and_flags_literals() {
        let yaml = "\
on: push
jobs:
  build:
    env:
      GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
      NPM_TOKEN: ${{ env.NPM_TOKEN }}
      DB_PASSWORD: hunter2
";
        let diags = run(yaml);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("${{ secrets.DB_PASSWORD }}"));
    }

    #[test]
    fn ignores_non_secret_keys() {
        let yaml = "on: push\njobs:\n  build:\n    env:\n      NODE_VERSION: 20";
        assert!(run(yaml).is_empty());
    }
}
