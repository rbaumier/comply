use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["run_instruction"] => |node, source, ctx, diagnostics|
    let shell_text = shell_command_text(node, source);
    if !shell_text.contains('|') { return; }
    // Heuristic: ignore `||` (logical or) — only flag actual pipes.
    if !has_real_pipe(shell_text) { return; }

    // Look back for a SHELL instruction containing pipefail.
    let mut prev = node.prev_sibling();
    while let Some(p) = prev {
        if p.kind() == "shell_instruction" {
            let text = p.utf8_text(source).unwrap_or("");
            if text.contains("pipefail") { return; }
        }
        prev = p.prev_sibling();
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "Piped RUN without `pipefail`; upstream failures are silently ignored.".into(),
        severity: Severity::Warning,
        span: Some((node.byte_range().start, node.byte_range().len())),
    });
}

fn has_real_pipe(s: &str) -> bool {
    let bytes = s.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] == b'|' {
            let prev_pipe = i > 0 && bytes[i - 1] == b'|';
            let next_pipe = i + 1 < bytes.len() && bytes[i + 1] == b'|';
            if !prev_pipe && !next_pipe {
                return true;
            }
        }
    }
    false
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
    fn flags_pipe_without_pipefail() {
        assert_eq!(
            run("FROM node:20\nRUN curl https://example.com | tar xz\n").len(),
            1
        );
    }

    #[test]
    fn allows_pipe_with_shell_pipefail() {
        assert!(
            run("FROM node:20\nSHELL [\"/bin/bash\", \"-o\", \"pipefail\", \"-c\"]\nRUN curl https://example.com | tar xz\n").is_empty()
        );
    }

    #[test]
    fn allows_run_without_pipe() {
        assert!(run("FROM node:20\nRUN echo hello\n").is_empty());
    }
}
