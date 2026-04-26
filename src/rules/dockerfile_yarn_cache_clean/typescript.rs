use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "run_instruction" { return; }
    let shell_text = shell_command_text(node, source);
    if !shell_text.contains("yarn install") { return; }
    if shell_text.contains("yarn cache clean") { return; }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "`yarn install` must be paired with `yarn cache clean` in the same RUN.".into(),
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
    fn flags_yarn_install_without_cache_clean() {
        assert_eq!(run("RUN yarn install\n").len(), 1);
    }

    #[test]
    fn allows_yarn_install_with_cache_clean() {
        assert!(run("RUN yarn install && yarn cache clean\n").is_empty());
    }
}
