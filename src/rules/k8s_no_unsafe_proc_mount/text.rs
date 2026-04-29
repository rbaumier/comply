//! k8s-no-unsafe-proc-mount tree-sitter backend (YAML AST).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) { return; }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    let Some(pod_spec) = y::pod_spec_mapping(node, source, &kind) else { return; };
    for container in y::containers_of_pod_spec(pod_spec, source, true) {
        let Some(sc) = y::descend_mapping(container, source, &["securityContext"]) else { continue; };
        let Some(pair) = y::find_pair(sc, source, "procMount") else { continue; };
        if y::pair_scalar_value(pair, source).as_deref() == Some("Unmasked") {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &pair,
                super::META.id,
                "Container uses `procMount: Unmasked`; revert to the default masked /proc.".into(),
                Severity::Warning,
            ));
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
    fn flags_unmasked() {
        let yaml = "apiVersion: v1\nkind: Pod\nspec:\n  containers:\n  - name: app\n    image: nginx:1.0\n    securityContext:\n      procMount: Unmasked";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_default() {
        let yaml = "apiVersion: v1\nkind: Pod\nspec:\n  containers:\n  - name: app\n    image: nginx:1.0\n    securityContext:\n      procMount: Default";
        assert!(run(yaml).is_empty());
    }
}
