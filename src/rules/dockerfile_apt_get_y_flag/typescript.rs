//! dockerfile-apt-get-y-flag tree-sitter backend.

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
    let has_yes = text.contains(" -y")
        || text.contains("--yes")
        || text.contains("--assume-yes")
        || text.contains(" -qq");
    if has_yes { return; }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "`apt-get install` must use `-y` to run non-interactively.".into(),
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
    fn flags_install_without_yes() {
        assert_eq!(run("RUN apt-get install curl\n").len(), 1);
    }

    #[test]
    fn allows_install_with_yes() {
        assert!(run("RUN apt-get install -y curl\n").is_empty());
    }
}
