//! k8s-no-writable-host-mount tree-sitter backend (YAML AST).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) { return; }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    let Some(pod_spec) = y::pod_spec_mapping(node, source, &kind) else { return; };
    let Some(volumes) = y::descend_sequence(pod_spec, source, &["volumes"]) else { return; };
    for volume_map in y::sequence_item_mappings(volumes) {
        if y::find_pair(volume_map, source, "hostPath").is_none() {
            continue;
        }
        let name = y::find_pair(volume_map, source, "name")
            .and_then(|p| y::pair_scalar_value(p, source));
        let writable = match &name {
            Some(n) => !volume_mount_is_readonly(pod_spec, source, n),
            None => true,
        };
        if writable {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &volume_map,
                super::META.id,
                "Pod uses a writable hostPath volume; mark the volumeMount as `readOnly: true` or remove it.".into(),
                Severity::Warning,
            ));
        }
    }
}

fn volume_mount_is_readonly(pod_spec: tree_sitter::Node, source: &[u8], vol_name: &str) -> bool {
    let mut found_mount = false;
    for container in y::containers_of_pod_spec(pod_spec, source, true) {
        let Some(mounts) = y::descend_sequence(container, source, &["volumeMounts"]) else {
            continue;
        };
        for mount_map in y::sequence_item_mappings(mounts) {
            let Some(name_pair) = y::find_pair(mount_map, source, "name") else {
                continue;
            };
            let Some(mname) = y::pair_scalar_value(name_pair, source) else {
                continue;
            };
            if mname == vol_name {
                found_mount = true;
                let ro = y::find_pair(mount_map, source, "readOnly")
                    .and_then(|p| y::pair_scalar_value(p, source));
                if ro.as_deref() != Some("true") {
                    return false;
                }
            }
        }
    }
    found_mount
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_yaml;

    fn run(s: &str) -> Vec<Diagnostic> {
        run_yaml(s, &Check)
    }

    #[test]
    fn flags_writable_host_path() {
        let yaml = "apiVersion: v1\nkind: Pod\nspec:\n  volumes:\n  - name: host-vol\n    hostPath:\n      path: /data\n  containers:\n  - name: app\n    image: nginx:1.0";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_no_host_path() {
        let yaml =
            "apiVersion: v1\nkind: Pod\nspec:\n  containers:\n  - name: app\n    image: nginx:1.0";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn allows_host_path_mounted_readonly() {
        let yaml = "apiVersion: v1\nkind: Pod\nspec:\n  volumes:\n  - name: host-vol\n    hostPath:\n      path: /data\n  containers:\n  - name: app\n    image: nginx:1.0\n    volumeMounts:\n    - name: host-vol\n      mountPath: /data\n      readOnly: true";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn flags_host_path_mounted_writable() {
        let yaml = "apiVersion: v1\nkind: Pod\nspec:\n  volumes:\n  - name: host-vol\n    hostPath:\n      path: /data\n  containers:\n  - name: app\n    image: nginx:1.0\n    volumeMounts:\n    - name: host-vol\n      mountPath: /data";
        assert_eq!(run(yaml).len(), 1);
    }
}
