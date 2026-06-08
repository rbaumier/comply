//! dockerfile-single-healthcheck tree-sitter backend.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["healthcheck_instruction"] => |node, source, ctx, diagnostics|
    let _ = source;
    let mut prev = node.prev_sibling();
    while let Some(sibling) = prev {
        if sibling.kind() == "healthcheck_instruction" {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: super::META.id.into(),
                message: "Multiple HEALTHCHECK instructions found; only the last one takes effect.".into(),
                severity: Severity::Warning,
                span: Some((node.byte_range().start, node.byte_range().len())),
            });
            return;
        }
        prev = sibling.prev_sibling();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_dockerfile(s, &Check)
    }

    #[test]
    fn flags_duplicate_healthcheck() {
        let src =
            "FROM node:20\nHEALTHCHECK CMD curl localhost\nHEALTHCHECK CMD curl localhost:8080\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_single_healthcheck() {
        let src = "FROM node:20\nHEALTHCHECK CMD curl localhost\n";
        assert!(run(src).is_empty());
    }
}
