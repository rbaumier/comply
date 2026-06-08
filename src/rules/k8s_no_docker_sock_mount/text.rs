//! k8s-no-docker-sock-mount tree-sitter backend (YAML AST).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) { return; }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    let Some(pod_spec) = y::pod_spec_mapping(node, source, &kind) else { return; };
    let Some(volumes) = y::descend_sequence(pod_spec, source, &["volumes"]) else { return; };
    for volume_map in y::sequence_item_mappings(volumes) {
        let Some(host_path) = y::descend_mapping(volume_map, source, &["hostPath"]) else { continue; };
        let Some(path_pair) = y::find_pair(host_path, source, "path") else { continue; };
        let Some(path) = y::pair_scalar_value(path_pair, source) else { continue; };
        if path.contains("docker.sock") {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &volume_map,
                super::META.id,
                "Volume mounts the docker socket from the host; this grants full root on the node.".into(),
                Severity::Warning,
            ));
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
    fn flags_docker_sock() {
        let yaml = "apiVersion: v1\nkind: Pod\nspec:\n  volumes:\n  - name: docker\n    hostPath:\n      path: /var/run/docker.sock\n  containers:\n  - name: app\n    image: nginx:1.0";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_no_volumes() {
        let yaml =
            "apiVersion: v1\nkind: Pod\nspec:\n  containers:\n  - name: app\n    image: nginx:1.0";
        assert!(run(yaml).is_empty());
    }
}
