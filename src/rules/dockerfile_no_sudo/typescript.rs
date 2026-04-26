//! dockerfile-no-sudo tree-sitter backend.

use crate::diagnostic::{Diagnostic, Severity};

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

fn contains_sudo(text: &str) -> bool {
    let bytes = text.as_bytes();
    let needle = b"sudo";
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            let before_ok = i == 0 || !is_word_byte(bytes[i - 1]);
            let after_ok =
                i + needle.len() == bytes.len() || !is_word_byte(bytes[i + needle.len()]);
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
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
    if contains_sudo(text) {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message: "Avoid `sudo` inside a Dockerfile RUN; use USER instead.".into(),
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
    fn flags_sudo() {
        assert_eq!(run("RUN sudo apt-get install curl\n").len(), 1);
    }

    #[test]
    fn allows_no_sudo() {
        assert!(run("RUN apt-get install curl\n").is_empty());
    }
}
