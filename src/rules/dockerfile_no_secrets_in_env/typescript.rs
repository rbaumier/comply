//! dockerfile-no-secrets-in-env tree-sitter backend.

use crate::diagnostic::{Diagnostic, Severity};

const SECRET_SUBSTRINGS: &[&str] = &["SECRET", "TOKEN", "PASSWORD", "PASSWD", "APIKEY"];

crate::ast_check! { on ["env_instruction"] => |node, source, ctx, diagnostics|
    // Iterate env_pair children. Each env_pair has either:
    //   unquoted_string `=` unquoted_string  (KEY=VALUE form)
    //   unquoted_string unquoted_string      (legacy KEY VALUE form)
    for i in 0..node.child_count() {
        let pair = node.child(i).unwrap();
        if pair.kind() != "env_pair" { continue; }
        let mut key: Option<&str> = None;
        let mut value: Option<&str> = None;
        for j in 0..pair.child_count() {
            let c = pair.child(j).unwrap();
            if c.kind() == "unquoted_string" {
                let text = std::str::from_utf8(&source[c.byte_range()]).unwrap_or("");
                if key.is_none() {
                    key = Some(text);
                } else if value.is_none() {
                    value = Some(text);
                }
            }
        }
        let Some(key) = key else { continue; };
        let value = value.unwrap_or("");
        if !has_secret_marker(key) { continue; }
        if value.is_empty() { continue; }
        if is_pure_var_ref(value) { continue; }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message: format!("ENV `{key}` embeds a secret-like literal in the image layer."),
            severity: Severity::Error,
            span: Some((node.byte_range().start, node.byte_range().len())),
        });
        break;
    }
}

fn has_secret_marker(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    // `KEY` alone is too noisy (e.g. `ENV PRIMARY_KEY_TYPE=...`); require the
    // longer markers to reduce false positives.
    SECRET_SUBSTRINGS.iter().any(|m| upper.contains(m))
        || upper.ends_with("_KEY")
        || upper == "KEY"
}

fn is_pure_var_ref(value: &str) -> bool {
    let v = value.trim().trim_matches('"').trim_matches('\'');
    v.starts_with('$')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_dockerfile(s, &Check)
    }

    #[test]
    fn flags_env_api_key() {
        assert_eq!(run("ENV API_KEY=sk-live-abc123\n").len(), 1);
    }

    #[test]
    fn flags_env_password_legacy_form() {
        assert_eq!(run("ENV DB_PASSWORD hunter2\n").len(), 1);
    }

    #[test]
    fn allows_non_secret_env() {
        assert!(run("ENV NODE_ENV=production\n").is_empty());
    }

    #[test]
    fn allows_var_passthrough() {
        assert!(run("ENV API_TOKEN=$API_TOKEN\n").is_empty());
    }
}
