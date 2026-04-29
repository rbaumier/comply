//! k8s-probe-port-exists tree-sitter backend (YAML AST).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;
use tree_sitter::Node;

fn collect_container_ports(container: Node, source: &[u8]) -> (Vec<u32>, Vec<String>) {
    let mut numbers = Vec::new();
    let mut names = Vec::new();
    let Some(ports) = y::descend_sequence(container, source, &["ports"]) else {
        return (numbers, names);
    };
    for port_map in y::sequence_item_mappings(ports) {
        if let Some(cp_pair) = y::find_pair(port_map, source, "containerPort")
            && let Some(cp_str) = y::pair_scalar_value(cp_pair, source)
            && let Ok(cp) = cp_str.trim().parse::<u32>()
        {
            numbers.push(cp);
        }
        if let Some(name_pair) = y::find_pair(port_map, source, "name")
            && let Some(name) = y::pair_scalar_value(name_pair, source)
        {
            names.push(name);
        }
    }
    (numbers, names)
}

fn check_probe(
    container: Node,
    source: &[u8],
    probe_key: &str,
    numbers: &[u32],
    names: &[String],
    ctx_path: &std::path::Path,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Try httpGet.port then tcpSocket.port.
    for sub in ["httpGet", "tcpSocket"] {
        let Some(target) = y::descend_mapping(container, source, &[probe_key, sub]) else {
            continue;
        };
        let Some(port_pair) = y::find_pair(target, source, "port") else {
            continue;
        };
        let Some(port_val) = y::pair_scalar_value(port_pair, source) else {
            continue;
        };
        let trimmed = port_val.trim();
        let matched = if let Ok(n) = trimmed.parse::<u32>() {
            numbers.contains(&n)
        } else {
            names.iter().any(|nm| nm == trimmed)
        };
        if !matched {
            diagnostics.push(Diagnostic::at_node(
                ctx_path,
                &port_pair,
                super::META.id,
                format!(
                    "{} port `{}` does not match any port declared by the container.",
                    probe_key, trimmed
                ),
                Severity::Warning,
            ));
        }
    }
}

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) { return; }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    let Some(pod_spec) = y::pod_spec_mapping(node, source, &kind) else { return; };
    for container in y::containers_of_pod_spec(pod_spec, source, true) {
        let (numbers, names) = collect_container_ports(container, source);
        for probe_key in ["livenessProbe", "readinessProbe", "startupProbe"] {
            check_probe(container, source, probe_key, &numbers, &names, ctx.path, diagnostics);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_yaml;

    fn run(s: &str) -> Vec<Diagnostic> {
        run_yaml(s, &Check)
    }

    #[test]
    fn flags_mismatched_probe_port() {
        let yaml = "apiVersion: v1\nkind: Pod\nspec:\n  containers:\n  - name: app\n    image: nginx:1.0\n    ports:\n    - containerPort: 8080\n      name: http\n    livenessProbe:\n      httpGet:\n        path: /\n        port: 9090";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_matching_numeric_port() {
        let yaml = "apiVersion: v1\nkind: Pod\nspec:\n  containers:\n  - name: app\n    image: nginx:1.0\n    ports:\n    - containerPort: 8080\n      name: http\n    livenessProbe:\n      httpGet:\n        path: /\n        port: 8080";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn allows_matching_named_port() {
        let yaml = "apiVersion: v1\nkind: Pod\nspec:\n  containers:\n  - name: app\n    image: nginx:1.0\n    ports:\n    - containerPort: 8080\n      name: http\n    readinessProbe:\n      httpGet:\n        path: /\n        port: http";
        assert!(run(yaml).is_empty());
    }
}
