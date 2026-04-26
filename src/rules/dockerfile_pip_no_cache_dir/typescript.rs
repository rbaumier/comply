use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["run_instruction"] => |node, source, ctx, diagnostics|
    let shell_text = shell_command_text(node, source);
    let mentions_pip = shell_text.contains("pip install") || shell_text.contains("pip3 install");
    if !mentions_pip { return; }
    if shell_text.contains("--no-cache-dir") { return; }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "`pip install` must pass `--no-cache-dir` in Dockerfiles.".into(),
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
    fn flags_pip_install_without_no_cache_dir() {
        assert_eq!(run("RUN pip install flask\n").len(), 1);
    }

    #[test]
    fn allows_pip_install_with_no_cache_dir() {
        assert!(run("RUN pip install --no-cache-dir flask\n").is_empty());
    }
}
