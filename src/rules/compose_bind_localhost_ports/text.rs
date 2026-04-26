//! compose-bind-localhost-ports text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers::{as_sequence, pair_key_text, pair_value_node};

/// Common database / cache / broker ports that should never be published on
/// `0.0.0.0`.
const SENSITIVE_PORTS: &[&str] = &[
    "5432",  // postgres
    "3306",  // mysql / mariadb
    "1433",  // mssql
    "6379",  // redis
    "11211", // memcached
    "27017", // mongo
    "9200",  // elasticsearch
    "5672",  // rabbitmq
    "9092",  // kafka
];

/// From a published port spec like `127.0.0.1:5432:5432`, `5432:5432`, or
/// `5432`, return the host-side port (first colon-separated numeric segment
/// after the optional IP prefix).
fn extract_host_port(spec: &str) -> Option<&str> {
    let spec = if let Some(rest) = spec.strip_prefix("127.0.0.1:") {
        rest
    } else if let Some(rest) = spec.strip_prefix("0.0.0.0:") {
        rest
    } else {
        spec
    };
    let first = spec.split(':').next()?;
    if first.chars().all(|c| c.is_ascii_digit()) {
        Some(first)
    } else {
        None
    }
}

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

crate::ast_check! { on ["block_mapping_pair"] => |node, source, ctx, diagnostics|
    if pair_key_text(node, source).as_deref() != Some("ports") { return; }
    if !looks_like_compose(ctx.path, ctx.source) { return; }

    let Some(value) = pair_value_node(node) else { return; };
    let Some(seq) = as_sequence(value) else { return; };
    let mut cursor = seq.walk();
    for item in seq.named_children(&mut cursor) {
        if item.kind() != "block_sequence_item" { continue; }
        // Extract the flow_node inside the item.
        let mut icur = item.walk();
        let Some(scalar_node) = item
            .named_children(&mut icur)
            .find(|c| c.kind() == "flow_node") else { continue; };
        let raw = scalar_node
            .utf8_text(source)
            .unwrap_or("")
            .trim()
            .trim_matches('"')
            .trim_matches('\'');
        if raw.is_empty() { continue; }
        let Some(host_port) = extract_host_port(raw) else { continue; };
        if !SENSITIVE_PORTS.contains(&host_port) { continue; }
        if raw.starts_with("127.0.0.1:") { continue; }

        let pos = scalar_node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: format!(
                "Port `{host_port}` exposed on all interfaces; bind it on `127.0.0.1:` instead."
            ),
            severity: Severity::Warning,
            span: Some((scalar_node.byte_range().start, scalar_node.byte_range().len())),
        });
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
    fn flags_postgres_published_on_all_interfaces() {
        let src = "services:\n  db:\n    ports:\n      - \"5432:5432\"\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_localhost_bound_port() {
        let src = "services:\n  db:\n    ports:\n      - \"127.0.0.1:5432:5432\"\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_sensitive_ports() {
        let src = "services:\n  web:\n    ports:\n      - \"8080:8080\"\n";
        assert!(run(src).is_empty());
    }
}
