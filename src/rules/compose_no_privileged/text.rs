//! compose-no-privileged text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers::{pair_key_text, pair_scalar_value};

fn looks_like_compose(path: &std::path::Path, source: &str) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name.contains("compose") {
        return true;
    }
    source.lines().any(|l| l == "services:" || l.starts_with("services:"))
}

crate::ast_check! { on ["block_mapping_pair"] => |node, source, ctx, diagnostics|
    if pair_key_text(node, source).as_deref() != Some("privileged") { return; }
    if !looks_like_compose(ctx.path, ctx.source) { return; }

    let Some(value) = pair_scalar_value(node, source) else { return; };
    let value = value.trim().trim_matches('#').trim();
    if !(value.eq_ignore_ascii_case("true") || value == "yes") { return; }

    let value_node = node.named_child(1).unwrap_or(node);
    let pos = value_node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: 1,
        rule_id: super::META.id.into(),
        message: "`privileged: true` grants kernel-level access; use `cap_add:` for the minimum needed.".into(),
        severity: Severity::Error,
        span: Some((value_node.byte_range().start, value_node.byte_range().len())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use crate::rules::test_helpers::run_yaml_with_path;

    fn run(source: &str) -> Vec<Diagnostic> {
        run_yaml_with_path(source, &Check, "docker-compose.yml")
    }

    #[test]
    fn flags_privileged_true() {
        let src = "services:\n  db:\n    privileged: true\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_privileged_false() {
        let src = "services:\n  db:\n    privileged: false\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_unset() {
        assert!(run("services:\n  db:\n    image: postgres:16.6\n").is_empty());
    }
}
