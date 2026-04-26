//! dockerfile-no-cd-in-run tree-sitter backend.

use crate::diagnostic::{Diagnostic, Severity};

fn segment_starts_with_cd(segment: &str) -> bool {
    let trimmed = segment.trim_start();
    if let Some(rest) = trimmed.strip_prefix("cd ") {
        return !rest.trim().is_empty();
    }
    trimmed == "cd"
}

fn has_cd_command(text: &str) -> bool {
    for segment in text.split(|c| matches!(c, '\n' | ';' | '&' | '|')) {
        if segment_starts_with_cd(segment) {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["run_instruction"] => |node, source, ctx, diagnostics|
    let mut shell_text: Option<&str> = None;
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        if child.kind() == "shell_command" {
            shell_text = std::str::from_utf8(&source[child.byte_range()]).ok();
            break;
        }
    }
    let Some(text) = shell_text else { return; };
    if has_cd_command(text) {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message: "Use WORKDIR instead of `cd` inside RUN.".into(),
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
    fn flags_cd_in_run() {
        assert_eq!(run("RUN cd /app && make\n").len(), 1);
    }

    #[test]
    fn allows_run_without_cd() {
        assert!(run("RUN make\n").is_empty());
    }

    #[test]
    fn does_not_match_words_containing_cd() {
        assert!(run("RUN echo abcd\n").is_empty());
    }
}
