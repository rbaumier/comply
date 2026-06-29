//! Flags `Command::new(...)` executions (`.output()`/`.spawn()`/`.status()`)
//! that have no nearby timeout, as they can hang indefinitely.
//!
//! Not flagged:
//! - A `.spawn()` with no output capture (`.output()`/`.stdout(`/`.stderr(`):
//!   stdio is inherited, so it is an interactive foreground process (editor,
//!   shell, user script) where a timeout would be wrong.
//! - `build.rs` build scripts and test contexts (a `#[cfg(test)]` module /
//!   `#[test]` fn, or a file under a `tests/` integration directory): they
//!   never run in production, so the timeout requirement does not apply.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["Command::new"] => |node, source, ctx, diagnostics|
    if ctx.path.file_name() == Some(std::ffi::OsStr::new("build.rs")) { return; }
    // `Command::new()` inside a `#[cfg(test)]` module / `#[test]` fn, or under a
    // `tests/` integration directory, is compile-time test setup that never runs
    // in production, so the timeout requirement does not apply.
    if crate::rules::rust_helpers::is_in_test_context(node, source)
        || crate::rules::rust_helpers::is_under_tests_dir(ctx.path)
    {
        return;
    }
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

    // `.spawn()` with no output capture (`.output()`/`.stdout(`/`.stderr(`) inherits
    // the parent's stdio: an interactive foreground launch (editor, shell, user
    // script) that may run arbitrarily long under user control. A timeout would
    // kill it mid-use, so it is not the unguarded-background-command this rule
    // targets.
    if context_str.contains(".spawn()")
        && !context_str.contains(".output()")
        && !context_str.contains(".stdout(")
        && !context_str.contains(".stderr(")
    {
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
    fn allows_bare_interactive_spawn() {
        let src = "fn f() { Command::new(\"ls\").spawn().unwrap(); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_command_spawn_with_piped_stdout() {
        let src = "fn f() { Command::new(\"ls\").stdout(Stdio::piped()).spawn().unwrap(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_interactive_spawn_with_inherited_stdio_issue_6701() {
        let src = "fn f() {\n    let r = Command::new(exe)\n        .args(args.iter())\n        .spawn()\n        .and_then(|mut p| p.wait());\n}";
        assert!(run(src).is_empty());
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
    fn skips_command_in_cfg_test_module() {
        let src = r#"#[cfg(test)]
mod test {
    fn helper() { Command::new("git").args(["status"]).output().ok(); }
}"#;
        assert!(
            run(src).is_empty(),
            "Command::new inside a #[cfg(test)] module is a false positive"
        );
    }

    #[test]
    fn skips_command_in_test_fn() {
        let src = r#"#[cfg(test)]
mod t {
    #[test]
    fn it_works() { Command::new("git").output().ok(); }
}"#;
        assert!(
            run(src).is_empty(),
            "Command::new inside a #[test] fn is a false positive"
        );
    }

    #[test]
    fn skips_command_under_tests_dir() {
        let src = r#"fn f() { Command::new("git").output().ok(); }"#;
        assert!(
            crate::rules::test_helpers::run_rule(&Check, src, "tests/integration.rs").is_empty(),
            "Command::new under a tests/ directory is a false positive"
        );
    }

    #[test]
    fn flags_command_in_non_test_dir() {
        let src = r#"fn run_just() {
  let output = Command::new(JUST)
    .current_dir(work.path())
    .output()
    .unwrap();
  assert!(!output.status.success());
}"#;
        let diags = crate::rules::test_helpers::run_rule_gated(&Check, src, "src/main.rs");
        assert_eq!(
            diags.len(),
            1,
            "production Command::new outside a test context must still fire"
        );
    }
}
