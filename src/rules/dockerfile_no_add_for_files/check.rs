//! dockerfile-no-add-for-files tree-sitter backend.

use crate::diagnostic::{Diagnostic, Severity};

const ARCHIVE_SUFFIXES: &[&str] = &[
    ".tar", ".tar.gz", ".tgz", ".tar.bz2", ".tbz2", ".tar.xz", ".txz", ".zip",
];

fn is_archive(src: &str) -> bool {
    ARCHIVE_SUFFIXES.iter().any(|s| src.ends_with(s))
}

fn is_remote(src: &str) -> bool {
    src.starts_with("http://") || src.starts_with("https://")
}

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
    if is_archive(trimmed) || is_remote(trimmed) { return; }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "Use COPY instead of ADD for plain files.".into(),
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
    fn flags_add_for_plain_file() {
        assert_eq!(run("ADD package.json /app/\n").len(), 1);
    }

    #[test]
    fn allows_add_for_url() {
        assert!(run("ADD https://example.com/file.tar.gz /app/\n").is_empty());
    }

    #[test]
    fn allows_add_for_local_archive() {
        assert!(run("ADD app.tar.gz /app/\n").is_empty());
    }
}
