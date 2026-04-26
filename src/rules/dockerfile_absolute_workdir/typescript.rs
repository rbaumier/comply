//! dockerfile-absolute-workdir tree-sitter backend.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "workdir_instruction" { return; }
    let mut path_text: Option<&str> = None;
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        if child.kind() == "path" {
            path_text = std::str::from_utf8(&source[child.byte_range()]).ok();
            break;
        }
    }
    let Some(text) = path_text else { return; };
    let trimmed = text.trim();
    if trimmed.is_empty() { return; }
    if trimmed.starts_with('/') || trimmed.starts_with('$') { return; }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "WORKDIR must use an absolute path.".into(),
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
    fn flags_relative_path() {
        assert_eq!(run("WORKDIR relative/path\n").len(), 1);
    }

    #[test]
    fn allows_absolute_path() {
        assert!(run("WORKDIR /absolute/path\n").is_empty());
    }

    #[test]
    fn allows_env_var() {
        assert!(run("WORKDIR $HOME/app\n").is_empty());
    }
}
