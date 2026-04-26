//! dockerfile-add-for-archive-extract tree-sitter backend.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["add_instruction"] => |node, source, ctx, diagnostics|
    let mut first_path: Option<&str> = None;
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        if child.kind() == "path" {
            first_path = std::str::from_utf8(&source[child.byte_range()]).ok();
            break;
        }
    }
    let Some(src) = first_path else { return; };
    let trimmed = src.trim();
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) { return; }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "Use RUN curl/wget + tar instead of ADD <url> to avoid leaving the archive in the layer.".into(),
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
    fn flags_add_url() {
        assert_eq!(run("ADD https://example.com/app.tar.gz /app/\n").len(), 1);
    }

    #[test]
    fn allows_add_local_archive() {
        assert!(run("ADD app.tar.gz /app/\n").is_empty());
    }
}
