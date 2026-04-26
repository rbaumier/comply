//! dockerfile-useradd-low-uid tree-sitter backend.
//!
//! Flags `RUN useradd …` invocations that don't pass `-l` / `--no-log-init`,
//! which is required to avoid sparse `/var/log/lastlog` files when high UIDs
//! are used.

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
    if !has_word(text, "useradd") { return; }
    if has_word(text, "-l") || has_word(text, "--no-log-init") { return; }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "`useradd` without `-l`/`--no-log-init` can bloat the image with a sparse `/var/log/lastlog`.".into(),
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
    fn flags_useradd_without_l() {
        let src = "FROM alpine\nRUN useradd --uid 100000 alice\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_useradd_with_l() {
        let src = "FROM alpine\nRUN useradd -l --uid 100000 alice\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_useradd_with_no_log_init() {
        let src = "FROM alpine\nRUN useradd --no-log-init --uid 100000 alice\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_runs_without_useradd() {
        let src = "FROM alpine\nRUN apk add bash\n";
        assert!(run(src).is_empty());
    }
}
