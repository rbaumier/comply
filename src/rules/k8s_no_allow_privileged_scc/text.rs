//! k8s-no-allow-privileged-scc tree-sitter backend (YAML AST).
//!
//! Flags `kind: SecurityContextConstraints` manifests that declare
//! `allowPrivilegedContainer: true`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) {
        return;
    }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    if kind != "SecurityContextConstraints" {
        return;
    }
    let Some(pair) = y::find_pair(node, source, "allowPrivilegedContainer") else { return; };
    if y::pair_scalar_value(pair, source).as_deref() == Some("true") {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &pair,
            super::META.id,
            "SecurityContextConstraints must not set allowPrivilegedContainer: true.".into(),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_yaml;

    fn run(source: &str) -> Vec<Diagnostic> {
        run_yaml(source, &Check)
    }

    #[test]
    fn flags_allow_privileged_true() {
        let yaml = "apiVersion: security.openshift.io/v1\nkind: SecurityContextConstraints\nallowPrivilegedContainer: true";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_allow_privileged_false() {
        let yaml = "apiVersion: security.openshift.io/v1\nkind: SecurityContextConstraints\nallowPrivilegedContainer: false";
        assert!(run(yaml).is_empty());
    }
}
