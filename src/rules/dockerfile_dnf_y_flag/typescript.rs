use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["run_instruction"] prefilter = ["dnf install"] => |node, source, ctx, diagnostics|
    let shell_text = shell_command_text(node, source);
    if !shell_text.contains("dnf install") { return; }
    if shell_text.contains(" -y") || shell_text.contains("--assumeyes") { return; }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "`dnf install` must pass `-y` to run non-interactively.".into(),
        severity: Severity::Warning,
        span: Some((node.byte_range().start, node.byte_range().len())),
    });
}

fn shell_command_text<'a>(run: tree_sitter::Node, source: &'a [u8]) -> &'a str {
    let mut cursor = run.walk();
    for c in run.children(&mut cursor) {
        if c.kind() == "shell_command" {
            return c.utf8_text(source).unwrap_or("");
        }
    }
    ""
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_dockerfile(s, &Check)
    }

    #[test]
    fn flags_dnf_install_without_y() {
        assert_eq!(run("RUN dnf install wget\n").len(), 1);
    }

    #[test]
    fn allows_dnf_install_with_y() {
        assert!(run("RUN dnf install -y wget\n").is_empty());
    }
}
