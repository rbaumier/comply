//! k8s-no-privileged-ports tree-sitter backend (YAML AST).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) { return; }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    let Some(pod_spec) = y::pod_spec_mapping(node, source, &kind) else { return; };
    for container in y::containers_of_pod_spec(pod_spec, source, true) {
        let Some(ports) = y::descend_sequence(container, source, &["ports"]) else { continue; };
        for port_map in y::sequence_item_mappings(ports) {
            let Some(cp_pair) = y::find_pair(port_map, source, "containerPort") else { continue; };
            let Some(cp_str) = y::pair_scalar_value(cp_pair, source) else { continue; };
            let Ok(cp) = cp_str.trim().parse::<u32>() else { continue; };
            if cp < 1024 {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &cp_pair,
                    super::META.id,
                    "Container binds a privileged port (<1024); use a high port and expose via a Service.".into(),
                    Severity::Warning,
                ));
            }
        }
    }
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
    
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "manifest.yaml")
    }

    #[test]
    fn flags_port_80() {
        let yaml = "apiVersion: v1\nkind: Pod\nspec:\n  containers:\n  - name: app\n    image: nginx:1.0\n    ports:\n    - containerPort: 80";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_high_port() {
        let yaml = "apiVersion: v1\nkind: Pod\nspec:\n  containers:\n  - name: app\n    image: nginx:1.0\n    ports:\n    - containerPort: 8080";
        assert!(run(yaml).is_empty());
    }
}
