//! k8s-no-deprecated-extensions-api tree-sitter backend (YAML AST).
//!
//! Flags any manifest whose `apiVersion` starts with `extensions/v1beta`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) {
        return;
    }
    let Some(pair) = y::find_pair(node, source, "apiVersion") else { return; };
    let Some(value) = y::pair_scalar_value(pair, source) else { return; };
    if value.starts_with("extensions/v1beta") {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &pair,
            super::META.id,
            format!("apiVersion `{value}` is deprecated and removed; migrate to a stable group/version."),
            Severity::Warning,
        ));
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
    
    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "manifest.yaml")
    }

    #[test]
    fn flags_extensions_v1beta1() {
        let yaml = "apiVersion: extensions/v1beta1\nkind: Deployment";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_apps_v1() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment";
        assert!(run(yaml).is_empty());
    }
}
