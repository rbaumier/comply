//! k8s-no-unsafe-sysctls tree-sitter backend (YAML AST).
//!
//! Flags any pod-spec sysctl entry whose `name` falls in the unsafe set:
//! `kernel.msg*`, `kernel.sem*`, `kernel.shm*`, `fs.mqueue.*`, `net.*`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) {
        return;
    }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    let Some(pod_spec) = y::pod_spec_mapping(node, source, &kind) else { return; };
    let Some(sc) = y::descend_mapping(pod_spec, source, &["securityContext"]) else { return; };
    let Some(sysctls) = y::descend_sequence(sc, source, &["sysctls"]) else { return; };
    for entry in y::sequence_item_mappings(sysctls) {
        let Some(name_pair) = y::find_pair(entry, source, "name") else { continue; };
        let Some(name) = y::pair_scalar_value(name_pair, source) else { continue; };
        if is_unsafe_sysctl(&name) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &name_pair,
                super::META.id,
                format!("Sysctl `{name}` is unsafe and must not be set on a Pod."),
                Severity::Warning,
            ));
        }
    }
}

fn is_unsafe_sysctl(name: &str) -> bool {
    name.starts_with("kernel.msg")
        || name.starts_with("kernel.sem")
        || name.starts_with("kernel.shm")
        || name.starts_with("fs.mqueue.")
        || name.starts_with("net.")
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
    
    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "manifest.yaml")
    }

    #[test]
    fn flags_unsafe_kernel_shm() {
        let yaml = "apiVersion: v1\nkind: Pod\nspec:\n  securityContext:\n    sysctls:\n    - name: kernel.shm_rmid_forced\n      value: \"1\"\n  containers:\n  - name: app\n    image: nginx:1.0";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn flags_unsafe_net() {
        let yaml = "apiVersion: v1\nkind: Pod\nspec:\n  securityContext:\n    sysctls:\n    - name: net.core.somaxconn\n      value: \"1024\"\n  containers:\n  - name: app\n    image: nginx:1.0";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_no_sysctls() {
        let yaml =
            "apiVersion: v1\nkind: Pod\nspec:\n  containers:\n  - name: app\n    image: nginx:1.0";
        assert!(run(yaml).is_empty());
    }
}
