use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["Command::new"] => |node, source, ctx, diagnostics|
    let Some(func_node) = node.child_by_field_name("function") else { return };
    let Ok(func_text) = func_node.utf8_text(source) else { return };
    if func_text != "Command::new" { return; }

    let row = node.start_position().row;
    let src_str = std::str::from_utf8(source).unwrap_or("");
    let lines: Vec<&str> = src_str.lines().collect();

    let check_start = row;
    let check_end = (row + 5).min(lines.len());
    let context = &lines[check_start..check_end];
    let context_str: String = context.join("\n");

    if context_str.contains("timeout") || context_str.contains("Duration") || context_str.contains("exec_timeout") {
        return;
    }

    if !context_str.contains(".output()") && !context_str.contains(".spawn()") && !context_str.contains(".status()") {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`Command::new()` without a timeout — wrap with a timeout to prevent hanging.".into(),
        Severity::Error,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(s, &Check)
    }

    #[test]
    fn flags_command_without_timeout() {
        let src = "fn f() { Command::new(\"git\").arg(\"status\").output().ok(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_command_with_timeout() {
        let src = "fn f() { exec_timeout(&mut Command::new(\"git\"), Duration::from_secs(10)); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_command_with_duration() {
        let src = "fn f() {\nlet mut cmd = Command::new(\"git\");\nlet d = Duration::from_secs(5);\ncmd.output().ok();\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_command_spawn_without_timeout() {
        let src = "fn f() { Command::new(\"ls\").spawn().unwrap(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_command_without_output_or_spawn() {
        let src = "fn f() { let mut cmd = Command::new(\"git\"); cmd.arg(\"status\"); }";
        assert!(run(src).is_empty());
    }
}
