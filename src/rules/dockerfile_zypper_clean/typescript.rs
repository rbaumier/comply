use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["run_instruction"] => |node, source, ctx, diagnostics|
    let shell_text = shell_command_text(node, source);
    if !mentions_zypper_install(shell_text) { return; }
    if shell_text.contains("zypper clean") { return; }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "`zypper install` must be paired with `zypper clean` in the same RUN.".into(),
        severity: Severity::Warning,
        span: Some((node.byte_range().start, node.byte_range().len())),
    });
}

/// Detect `zypper [opts...] install ...` or `zypper [opts...] in ...` in
/// any order, since options like `-n` may sit between `zypper` and the
/// `install` subcommand.
fn mentions_zypper_install(s: &str) -> bool {
    let mut iter = s.split_whitespace().peekable();
    while let Some(tok) = iter.next() {
        if tok == "zypper" {
            // Walk subsequent tokens; the first non-flag is the subcommand.
            for next in iter.by_ref() {
                if next.starts_with('-') {
                    continue;
                }
                if next == "install" || next == "in" {
                    return true;
                }
                break;
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
    fn flags_zypper_install_without_clean() {
        assert_eq!(run("RUN zypper -n install vim\n").len(), 1);
    }

    #[test]
    fn allows_zypper_install_with_clean() {
        assert!(run("RUN zypper -n install vim && zypper clean\n").is_empty());
    }
}
