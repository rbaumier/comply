//! k8s-no-secrets-in-configmap tree-sitter backend (YAML AST).
//!
//! Flags any `ConfigMap` key under `data:` or `binaryData:` whose name
//! looks like a secret (PASSWORD / TOKEN / SECRET / APIKEY / API_KEY /
//! PRIVATE_KEY / *_KEY).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

const SECRET_NEEDLES: &[&str] = &[
    "PASSWORD",
    "TOKEN",
    "SECRET",
    "APIKEY",
    "API_KEY",
    "PRIVATE_KEY",
];

crate::ast_check! { |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) {
        return;
    }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    if kind != "ConfigMap" {
        return;
    }
    for section in ["data", "binaryData"] {
        let Some(section_pair) = y::find_pair(node, source, section) else { continue; };
        let Some(value) = y::pair_value_node(section_pair) else { continue; };
        let Some(mapping) = y::as_mapping(value) else { continue; };
        let mut cursor = mapping.walk();
        for child in mapping.named_children(&mut cursor) {
            if child.kind() != "block_mapping_pair" { continue; }
            let Some(key) = y::pair_key_text(child, source) else { continue; };
            if is_secret_key(&key) {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &child,
                    super::META.id,
                    format!("ConfigMap key `{key}` looks like a secret; move it to a Secret."),
                    Severity::Warning,
                ));
            }
        }
    }
}

fn is_secret_key(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    SECRET_NEEDLES.iter().any(|n| {
        if *n == "KEY" {
            upper == "KEY" || upper.ends_with("_KEY") || upper.starts_with("KEY_")
        } else {
            upper.contains(n)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_yaml;

    fn run(source: &str) -> Vec<Diagnostic> {
        run_yaml(source, &Check)
    }

    #[test]
    fn flags_password_key() {
        let yaml = "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: cm\ndata:\n  DB_PASSWORD: hunter2";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn flags_token_key() {
        let yaml = "apiVersion: v1\nkind: ConfigMap\ndata:\n  GITHUB_TOKEN: abc";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_normal_keys() {
        let yaml = "apiVersion: v1\nkind: ConfigMap\ndata:\n  LOG_LEVEL: info\n  PORT: \"8080\"";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn ignores_non_configmap() {
        let yaml = "apiVersion: v1\nkind: Service\ndata:\n  DB_PASSWORD: hunter2";
        assert!(run(yaml).is_empty());
    }
}
