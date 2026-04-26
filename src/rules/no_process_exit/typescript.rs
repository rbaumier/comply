use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    // Skip files with a shebang.
    if ctx.source.starts_with("#!") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(obj) = callee.child_by_field_name("object") else { return };
    if obj.utf8_text(source).unwrap_or("") != "process" {
        return;
    }

    let Some(prop) = callee.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "exit" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-process-exit".into(),
        message: "`process.exit()` terminates abruptly — throw an error instead.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_process_exit() {
        assert_eq!(run("process.exit(1);").len(), 1);
    }

    #[test]
    fn flags_process_exit_no_args() {
        assert_eq!(run("process.exit();").len(), 1);
    }

    #[test]
    fn allows_shebang_file() {
        assert!(run("#!/usr/bin/env node\nprocess.exit(1);").is_empty());
    }

    #[test]
    fn flags_in_conditional() {
        assert_eq!(run("if (err) process.exit(1);").len(), 1);
    }
}
