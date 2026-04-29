//! dockerfile-no-from-platform tree-sitter backend.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["from_instruction"] prefilter = ["--platform"] => |node, source, ctx, diagnostics|
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        if child.kind() != "param" { continue; }
        let Ok(text) = std::str::from_utf8(&source[child.byte_range()]) else { continue; };
        if text.starts_with("--platform") {
            let pos = child.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: super::META.id.into(),
                message: "Avoid `--platform` on FROM; it breaks multi-arch builds.".into(),
                severity: Severity::Warning,
                span: Some((child.byte_range().start, child.byte_range().len())),
            });
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_dockerfile(s, &Check)
    }

    #[test]
    fn flags_platform_param() {
        assert_eq!(run("FROM --platform=linux/amd64 node:20\n").len(), 1);
    }

    #[test]
    fn allows_no_platform_param() {
        assert!(run("FROM node:20\n").is_empty());
    }
}
