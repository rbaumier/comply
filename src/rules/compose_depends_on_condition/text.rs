//! compose-depends-on-condition text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers::{as_mapping, as_sequence, pair_key_text, pair_value_node};

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

/// `depends_on` is short-form when it's a flow sequence (`[db]`) or a block
/// sequence of scalars (`- db`). Long-form is a mapping (`db:\n condition: …`).
fn is_short_form(value: tree_sitter::Node<'_>) -> bool {
    // Flow form: flow_node wrapping flow_sequence.
    if value.kind() == "flow_node" {
        let mut cursor = value.walk();
        for child in value.named_children(&mut cursor) {
            if child.kind() == "flow_sequence" {
                return true;
            }
        }
    }
    // Block sequence form.
    if as_sequence(value).is_some() {
        return true;
    }
    false
}

crate::ast_check! { on ["block_mapping_pair"] => |node, source, ctx, diagnostics|
    if pair_key_text(node, source).as_deref() != Some("depends_on") { return; }
    if !looks_like_compose(ctx.path, ctx.source) { return; }

    let Some(value) = pair_value_node(node) else { return; };
    // Long-form (mapping) is fine. Only flag sequence forms.
    if as_mapping(value).is_some() { return; }
    if !is_short_form(value) { return; }

    let pos = value.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: 1,
        rule_id: super::META.id.into(),
        message: "`depends_on:` short form only waits for startup; use the long form with `condition: service_healthy`.".into(),
        severity: Severity::Warning,
        span: Some((value.byte_range().start, value.byte_range().len())),
    });
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
    fn flags_short_form_list() {
        let src = "services:\n  api:\n    depends_on:\n      - db\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_flow_style_list() {
        let src = "services:\n  api:\n    depends_on: [db, cache]\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_long_form() {
        let src =
            "services:\n  api:\n    depends_on:\n      db:\n        condition: service_healthy\n";
        assert!(run(src).is_empty());
    }
}
