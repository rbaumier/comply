//! compose-no-inline-secrets text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers::{as_mapping, as_sequence, pair_key_text, pair_scalar_value, pair_value_node};

const SECRET_SUBSTRINGS: &[&str] = &["SECRET", "TOKEN", "PASSWORD", "PASSWD", "APIKEY"];

fn looks_like_compose(path: &std::path::Path, source: &str) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name.contains("compose") {
        return true;
    }
    source.lines().any(|l| l == "services:" || l.starts_with("services:"))
}

fn is_secret_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    SECRET_SUBSTRINGS.iter().any(|m| upper.contains(m)) || upper.ends_with("_KEY")
}

fn is_var_ref(value: &str) -> bool {
    value.starts_with('$') || value.starts_with("${")
}

crate::ast_check! { on ["block_mapping_pair"] => |node, source, ctx, diagnostics|
    if pair_key_text(node, source).as_deref() != Some("environment") { return; }
    if !looks_like_compose(ctx.path, ctx.source) { return; }

    let Some(value) = pair_value_node(node) else { return; };

    // Mapping form: `environment:\n  KEY: value`.
    if let Some(env_map) = as_mapping(value) {
        let mut cursor = env_map.walk();
        for pair in env_map.named_children(&mut cursor) {
            if pair.kind() != "block_mapping_pair" { continue; }
            let Some(key) = pair_key_text(pair, source) else { continue; };
            if !is_secret_name(&key) { continue; }
            let Some(v) = pair_scalar_value(pair, source) else { continue; };
            if v.is_empty() || is_var_ref(&v) { continue; }
            let value_node = pair.named_child(1).unwrap_or(pair);
            let pos = value_node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "`environment.{key}` embeds a secret literal; move it to `env_file:` or `secrets:`."
                ),
                severity: Severity::Error,
                span: Some((value_node.byte_range().start, value_node.byte_range().len())),
            });
        }
        return;
    }

    // Sequence form: `environment:\n  - KEY=VALUE`.
    if let Some(seq) = as_sequence(value) {
        let mut cursor = seq.walk();
        for item in seq.named_children(&mut cursor) {
            if item.kind() != "block_sequence_item" { continue; }
            let mut icur = item.walk();
            let Some(item_value) = item.named_children(&mut icur).find(|c| c.kind() == "flow_node") else { continue; };
            let raw = item_value
                .utf8_text(source)
                .unwrap_or("")
                .trim()
                .trim_matches('"')
                .trim_matches('\'');
            let Some((key, val)) = raw.split_once('=') else { continue; };
            let key = key.trim();
            let val = val.trim();
            if !is_secret_name(key) { continue; }
            if val.is_empty() || is_var_ref(val) { continue; }
            let pos = item_value.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "`environment.{key}` embeds a secret literal; move it to `env_file:` or `secrets:`."
                ),
                severity: Severity::Error,
                span: Some((item_value.byte_range().start, item_value.byte_range().len())),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use crate::rules::test_helpers::run_yaml_with_path;

    fn run(source: &str) -> Vec<Diagnostic> {
        run_yaml_with_path(source, &Check, "docker-compose.yml")
    }

    #[test]
    fn flags_map_secret_literal() {
        let src = "services:\n  api:\n    environment:\n      API_TOKEN: sk-abc123\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_list_secret_literal() {
        let src = "services:\n  api:\n    environment:\n      - DB_PASSWORD=hunter2\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_var_passthrough() {
        let src = "services:\n  api:\n    environment:\n      API_TOKEN: ${API_TOKEN}\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_secret_env() {
        let src = "services:\n  api:\n    environment:\n      NODE_ENV: production\n";
        assert!(run(src).is_empty());
    }
}
