//! k8s-no-sensitive-host-mounts tree-sitter backend (YAML AST).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

const SENSITIVE: &[&str] = &[
    "/", "/boot", "/dev", "/etc", "/lib", "/proc", "/sys", "/usr",
];

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) { return; }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    let Some(pod_spec) = y::pod_spec_mapping(node, source, &kind) else { return; };
    let Some(volumes) = y::descend_sequence(pod_spec, source, &["volumes"]) else { return; };
    for volume_map in y::sequence_item_mappings(volumes) {
        let Some(host_path) = y::descend_mapping(volume_map, source, &["hostPath"]) else { continue; };
        let Some(path_pair) = y::find_pair(host_path, source, "path") else { continue; };
        let Some(path) = y::pair_scalar_value(path_pair, source) else { continue; };
        let trimmed = path.trim();
        let normalized = trimmed
            .trim_end_matches('/')
            .to_string();
        let is_sensitive = SENSITIVE.iter().any(|p| {
            if *p == "/" {
                trimmed == "/" || trimmed == "/."
            } else {
                normalized == *p || normalized.starts_with(&format!("{p}/"))
            }
        });
        if is_sensitive {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &volume_map,
                super::META.id,
                "Volume mounts a sensitive host path; this can compromise the node.".into(),
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
    fn flags_etc_mount() {
        let yaml = "apiVersion: v1\nkind: Pod\nspec:\n  volumes:\n  - name: root\n    hostPath:\n      path: /etc\n  containers:\n  - name: app\n    image: nginx:1.0";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_data_mount() {
        let yaml = "apiVersion: v1\nkind: Pod\nspec:\n  volumes:\n  - name: data\n    hostPath:\n      path: /data\n  containers:\n  - name: app\n    image: nginx:1.0";
        assert!(run(yaml).is_empty());
    }
}
