//! dockerfile-use-npm-ci tree-sitter backend.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["run_instruction"] => |node, source, ctx, diagnostics|
    // Find the shell_command child and read its full text.
    let mut shell_text: Option<&str> = None;
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        if child.kind() == "shell_command" {
            shell_text = std::str::from_utf8(&source[child.byte_range()]).ok();
            break;
        }
    }
    let Some(text) = shell_text else { return; };
    if text.contains("npm install") && !text.contains("npm ci") {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message: "Use `npm ci` in Dockerfiles for deterministic installs.".into(),
            severity: Severity::Warning,
            span: Some((node.byte_range().start, node.byte_range().len())),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_dockerfile(s, &Check)
    }

    #[test]
    fn flags_npm_install() {
        assert_eq!(run("RUN npm install\n").len(), 1);
    }

    #[test]
    fn allows_npm_ci() {
        assert!(run("RUN npm ci\n").is_empty());
    }
}
