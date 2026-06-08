//! dockerfile-apt-clean-lists tree-sitter backend.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["run_instruction"] prefilter = ["apt-get install"] => |node, source, ctx, diagnostics|
    let mut shell_text: Option<&str> = None;
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        if child.kind() == "shell_command" {
            shell_text = std::str::from_utf8(&source[child.byte_range()]).ok();
            break;
        }
    }
    let Some(text) = shell_text else { return; };
    if !text.contains("apt-get install") { return; }
    if text.contains("/var/lib/apt/lists") { return; }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "Add `rm -rf /var/lib/apt/lists/*` after apt-get install to keep the image small.".into(),
        severity: Severity::Warning,
        span: Some((node.byte_range().start, node.byte_range().len())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_dockerfile(s, &Check)
    }

    #[test]
    fn flags_install_without_clean() {
        assert_eq!(
            run("RUN apt-get update && apt-get install -y curl\n").len(),
            1
        );
    }

    #[test]
    fn allows_install_with_clean() {
        let src = "RUN apt-get update && apt-get install -y curl && rm -rf /var/lib/apt/lists/*\n";
        assert!(run(src).is_empty());
    }
}
