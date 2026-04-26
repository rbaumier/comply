//! dockerfile-exec-form-cmd tree-sitter backend.
//!
//! Flags `CMD` and `ENTRYPOINT` instructions that use the shell form
//! (`CMD node server.js`) instead of the exec form (`CMD ["node", "server.js"]`).
//! Shell form spawns `/bin/sh -c`, which doesn't forward signals to the
//! application — `docker stop` falls back to SIGKILL after the grace period.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["cmd_instruction", "entrypoint_instruction"] => |node, source, ctx, diagnostics|
    let _ = source;
    // Skip the cmd_instruction nested inside a healthcheck_instruction —
    // HEALTHCHECK's CMD argument is conventionally written in shell form.
    if let Some(parent) = node.parent()
        && parent.kind() == "healthcheck_instruction"
    {
        return;
    }
    let mut has_shell = false;
    let mut has_json = false;
    for i in 0..node.child_count() {
        match node.child(i).unwrap().kind() {
            "shell_command" => has_shell = true,
            "json_string_array" => has_json = true,
            _ => {}
        }
    }
    if has_shell && !has_json {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message: "CMD/ENTRYPOINT must use exec form (JSON array); shell form breaks signal forwarding.".into(),
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
    fn flags_shell_form_cmd() {
        assert_eq!(run("CMD node server.js\n").len(), 1);
    }

    #[test]
    fn flags_shell_form_entrypoint() {
        assert_eq!(run("ENTRYPOINT /entrypoint.sh\n").len(), 1);
    }

    #[test]
    fn allows_exec_form() {
        assert!(run("CMD [\"node\", \"server.js\"]\n").is_empty());
    }
}
