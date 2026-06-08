use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["run_instruction"] prefilter = ["zypper"] => |node, source, ctx, diagnostics|
    let shell_text = shell_command_text(node, source);
    let mentions_install = shell_text.contains("zypper install") || shell_text.contains("zypper in ");
    if !mentions_install { return; }
    if shell_text.contains(" -n") || shell_text.contains("--non-interactive") || shell_text.contains("zypper -n") {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "`zypper install` must pass `-n` or `--non-interactive`.".into(),
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
    fn flags_zypper_install_without_n() {
        assert_eq!(run("RUN zypper install vim\n").len(), 1);
    }

    #[test]
    fn allows_zypper_n_install() {
        assert!(run("RUN zypper -n install vim\n").is_empty());
    }
}
