//! compose-require-resource-limits text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers::{
    as_mapping, descend_mapping, find_pair, pair_key_text, pair_scalar_value, pair_value_node,
};

fn looks_like_compose(path: &std::path::Path, source: &str) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name.contains("compose") {
        return true;
    }
    source
        .lines()
        .any(|l| l == "services:" || l.starts_with("services:"))
}

fn service_has_memory_limit(svc_map: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let Some(limits) = descend_mapping(svc_map, source, &["deploy", "resources", "limits"]) else {
        return false;
    };
    let Some(memory_pair) = find_pair(limits, source, "memory") else {
        return false;
    };
    // Any non-empty memory value is accepted.
    pair_scalar_value(memory_pair, source)
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
}

crate::ast_check! { on ["block_mapping_pair"] => |node, source, ctx, diagnostics|
    if pair_key_text(node, source).as_deref() != Some("services") { return; }
    if !looks_like_compose(ctx.path, ctx.source) { return; }

    let Some(services_value) = pair_value_node(node) else { return; };
    let Some(services_map) = as_mapping(services_value) else { return; };

    let mut cursor = services_map.walk();
    for service_pair in services_map.named_children(&mut cursor) {
        if service_pair.kind() != "block_mapping_pair" { continue; }
        let Some(name) = pair_key_text(service_pair, source) else { continue; };
        let Some(svc_value) = pair_value_node(service_pair) else { continue; };
        let Some(svc_map) = as_mapping(svc_value) else { continue; };
        if service_has_memory_limit(svc_map, source) { continue; }

        let key = service_pair.named_child(0).unwrap_or(service_pair);
        let pos = key.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: format!(
                "Service `{name}` does not declare `deploy.resources.limits.memory`."
            ),
            severity: Severity::Warning,
            span: Some((key.byte_range().start, key.byte_range().len())),
        });
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
    use crate::diagnostic::Diagnostic;
    
    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "docker-compose.yml")
    }

    #[test]
    fn flags_service_without_memory_limit() {
        let src = "services:\n  api:\n    image: foo:1.0\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_service_with_memory_limit() {
        let src = "services:\n  api:\n    image: foo:1.0\n    deploy:\n      resources:\n        limits:\n          memory: 512M\n";
        assert!(run(src).is_empty());
    }
}
