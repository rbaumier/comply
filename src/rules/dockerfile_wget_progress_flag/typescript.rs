//! dockerfile-wget-progress-flag tree-sitter backend.

use crate::diagnostic::{Diagnostic, Severity};

fn has_word(text: &str, word: &str) -> bool {
    let bytes = text.as_bytes();
    let w = word.as_bytes();
    let mut i = 0;
    while i + w.len() <= bytes.len() {
        if &bytes[i..i + w.len()] == w {
            let left_ok = i == 0
                || matches!(
                    bytes[i - 1],
                    b' ' | b'\t' | b'\n' | b'&' | b'|' | b';' | b'`' | b'(' | b'\''
                );
            let right_idx = i + w.len();
            let right_ok = right_idx == bytes.len()
                || matches!(
                    bytes[right_idx],
                    b' ' | b'\t' | b'\n' | b'&' | b'|' | b';' | b'`' | b')' | b'\'' | b'='
                );
            if left_ok && right_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "run_instruction" { return; }
    let text = match std::str::from_utf8(&source[node.byte_range()]) {
        Ok(t) => t,
        Err(_) => return,
    };
    if !has_word(text, "wget") { return; }
    // Any of these flags suppresses the noisy default progress bar.
    if text.contains("--progress")
        || has_word(text, "--no-verbose")
        || has_word(text, "-nv")
        || has_word(text, "-q")
        || has_word(text, "--quiet")
    {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "`wget` without `--progress`/`--no-verbose` produces bloated build logs.".into(),
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
    fn flags_bare_wget() {
        let src = "FROM alpine\nRUN wget https://example.com/file.tgz\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_wget_with_progress() {
        let src = "FROM alpine\nRUN wget --progress=dot:giga https://example.com/file.tgz\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_wget_with_no_verbose() {
        let src = "FROM alpine\nRUN wget --no-verbose https://example.com/file.tgz\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_wget_with_quiet_short() {
        let src = "FROM alpine\nRUN wget -nv https://example.com/file.tgz\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_runs_without_wget() {
        let src = "FROM alpine\nRUN curl -fsSL https://example.com\n";
        assert!(run(src).is_empty());
    }
}
