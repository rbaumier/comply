//! dockerfile-no-apt-end-user tree-sitter backend.

use crate::diagnostic::{Diagnostic, Severity};

const APT_VERBS: &[&str] = &[
    "install",
    "update",
    "upgrade",
    "remove",
    "purge",
    "search",
    "show",
    "list",
    "autoremove",
];

fn uses_apt_end_user(text: &str) -> bool {
    for segment in text.split(|c: char| matches!(c, '\n' | ';' | '&' | '|')) {
        let mut tokens = segment.split_whitespace();
        let Some(first) = tokens.next() else {
            continue;
        };
        if first != "apt" {
            continue;
        }
        let Some(verb) = tokens.next() else {
            continue;
        };
        if APT_VERBS.contains(&verb) {
            return true;
        }
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
    if !uses_apt_end_user(text) { return; }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "Use `apt-get` instead of `apt` inside Dockerfiles; `apt`'s output is not stable.".into(),
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
    fn flags_apt_install() {
        assert_eq!(run("RUN apt install curl\n").len(), 1);
    }

    #[test]
    fn allows_apt_get() {
        assert!(run("RUN apt-get install -y curl\n").is_empty());
    }

    #[test]
    fn allows_apt_cache() {
        assert!(run("RUN apt-cache show curl\n").is_empty());
    }
}
