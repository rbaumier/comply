use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["Command::new"] => |node, source, ctx, diagnostics|
    if ctx.path.file_name() == Some(std::ffi::OsStr::new("build.rs")) { return; }
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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.rs")
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

    #[test]
    fn allows_command_in_build_rs() {
        let src = r#"fn commit_hash() { Command::new("git").args(["rev-parse", "--short", "HEAD"]).output().ok(); }"#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "build.rs").is_empty());
    }

    #[test]
    fn skips_command_in_integration_test_dir() {
        let src = r#"#[test]
fn invalid_justfile() {
  let output = Command::new(JUST)
    .current_dir(tmp.path())
    .output()
    .unwrap();
  assert!(!output.status.success());
}"#;
        let diags = crate::rules::test_helpers::run_rule_gated(&Check, src, "tests/edit.rs");
        assert!(
            diags.is_empty(),
            "Command::new without a timeout in an integration test file is a false positive"
        );
    }

    #[test]
    fn flags_command_in_non_test_dir() {
        let src = r#"#[test]
fn invalid_justfile() {
  let output = Command::new(JUST)
    .current_dir(tmp.path())
    .output()
    .unwrap();
  assert!(!output.status.success());
}"#;
        let diags = crate::rules::test_helpers::run_rule_gated(&Check, src, "src/main.rs");
        assert_eq!(
            diags.len(),
            1,
            "the same Command::new must still fire outside test directories"
        );
    }
}
