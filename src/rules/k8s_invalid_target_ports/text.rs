//! k8s-invalid-target-ports tree-sitter backend (YAML AST).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

fn is_valid_iana_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 15 {
        return false;
    }
    let bytes = name.as_bytes();
    let is_alnum_lower = |b: u8| b.is_ascii_digit() || b.is_ascii_lowercase();
    if !is_alnum_lower(bytes[0]) || !is_alnum_lower(bytes[bytes.len() - 1]) {
        return false;
    }
    for &b in bytes {
        if !(is_alnum_lower(b) || b == b'-') {
            return false;
        }
    }
    // Must contain at least one letter (IANA: not all digits).
    if !bytes.iter().any(|b| b.is_ascii_lowercase()) {
        return false;
    }
    true
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) { return; }
    let Some(kind) = y::manifest_kind(node, source) else { return; };

    // Container ports for workloads.
    if let Some(pod_spec) = y::pod_spec_mapping(node, source, &kind) {
        for container in y::containers_of_pod_spec(pod_spec, source, true) {
            let Some(ports) = y::descend_sequence(container, source, &["ports"]) else { continue; };
            for port_map in y::sequence_item_mappings(ports) {
                if let Some(name_pair) = y::find_pair(port_map, source, "name")
                    && let Some(name) = y::pair_scalar_value(name_pair, source)
                    && !is_valid_iana_name(&name)
                {
                    diagnostics.push(Diagnostic::at_node(
                        ctx.path,
                        &name_pair,
                        super::META.id,
                        format!("Port name `{}` does not conform to IANA naming.", name),
                        Severity::Warning,
                    ));
                }
            }
        }
    }

    // Service ports.
    if kind == "Service"
        && let Some(ports) = y::descend_sequence(node, source, &["spec", "ports"])
    {
        for port_map in y::sequence_item_mappings(ports) {
            if let Some(name_pair) = y::find_pair(port_map, source, "name")
                && let Some(name) = y::pair_scalar_value(name_pair, source)
                && !is_valid_iana_name(&name)
            {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &name_pair,
                    super::META.id,
                    format!("Port name `{}` does not conform to IANA naming.", name),
                    Severity::Warning,
                ));
            }
            // targetPort, when string-valued, must also match IANA.
            if let Some(tp_pair) = y::find_pair(port_map, source, "targetPort")
                && let Some(tp) = y::pair_scalar_value(tp_pair, source)
                && tp.parse::<u32>().is_err()
                && !is_valid_iana_name(&tp)
            {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &tp_pair,
                    super::META.id,
                    format!("targetPort `{}` does not conform to IANA naming.", tp),
                    Severity::Warning,
                ));
            }
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
    fn flags_uppercase_container_port_name() {
        let yaml = "apiVersion: v1\nkind: Pod\nspec:\n  containers:\n  - name: app\n    image: nginx:1.0\n    ports:\n    - containerPort: 8080\n      name: HTTP";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn flags_too_long_service_port_name() {
        let yaml = "apiVersion: v1\nkind: Service\nmetadata:\n  name: svc\nspec:\n  ports:\n  - name: this-is-way-too-long\n    port: 80\n    targetPort: 8080";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_valid_names() {
        let yaml = "apiVersion: v1\nkind: Pod\nspec:\n  containers:\n  - name: app\n    image: nginx:1.0\n    ports:\n    - containerPort: 8080\n      name: http";
        assert!(run(yaml).is_empty());
    }
}
