//! dockerfile-no-shell-utils-in-run tree-sitter backend.

use crate::diagnostic::{Diagnostic, Severity};

const FORBIDDEN: &[&str] = &[
    "ssh", "vim", "shutdown", "service", "ps", "free", "top", "kill", "mount", "ifconfig", "nano",
];

fn contains_word(text: &str, word: &str) -> bool {
    let bytes = text.as_bytes();
    let wb = word.as_bytes();
    let mut i = 0;
    while i + wb.len() <= bytes.len() {
        if &bytes[i..i + wb.len()] == wb {
            let before_ok = i == 0 || !is_word_byte(bytes[i - 1]);
            let after_ok = i + wb.len() == bytes.len() || !is_word_byte(bytes[i + wb.len()]);
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "run_instruction" { return; }
    let mut shell_text: Option<&str> = None;
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        if child.kind() == "shell_command" {
            shell_text = std::str::from_utf8(&source[child.byte_range()]).ok();
            break;
        }
    }
    let Some(text) = shell_text else { return; };
    for util in FORBIDDEN {
        if contains_word(text, util) {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: super::META.id.into(),
                message: format!("`{util}` is an interactive/system tool and should not be used inside RUN."),
                severity: Severity::Warning,
                span: Some((node.byte_range().start, node.byte_range().len())),
            });
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_dockerfile(s, &Check)
    }

    #[test]
    fn flags_ssh() {
        assert_eq!(run("RUN ssh user@host\n").len(), 1);
    }

    #[test]
    fn flags_top() {
        assert_eq!(run("RUN top -b -n 1\n").len(), 1);
    }

    #[test]
    fn allows_unrelated_command() {
        assert!(run("RUN apt-get install -y curl\n").is_empty());
    }
}
