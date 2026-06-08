//! no-shell-exec backend — flag `exec()` / `spawn()` / `execSync()` calls
//! that interpolate variables into a command string, or pass
//! `{ shell: true }`.

use crate::diagnostic::{Diagnostic, Severity};

const SHELL_FNS: &[&str] = &["exec", "execSync", "spawn", "spawnSync"];

fn tail_matches_shell_fn(name: &str) -> bool {
    let tail = name.rsplit('.').next().unwrap_or(name);
    SHELL_FNS.contains(&tail)
}

fn argument_uses_template_interpolation(arg: tree_sitter::Node) -> bool {
    if arg.kind() != "template_string" {
        return false;
    }
    let mut cursor = arg.walk();
    for child in arg.named_children(&mut cursor) {
        if child.kind() == "template_substitution" {
            return true;
        }
    }
    false
}

fn options_object_has_shell_true(arg: tree_sitter::Node, source: &[u8]) -> bool {
    if arg.kind() != "object" {
        return false;
    }
    let Ok(text) = arg.utf8_text(source) else {
        return false;
    };
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    compact.contains("shell:true")
}

crate::ast_check! { on ["call_expression"] prefilter = ["exec", "spawn"] => |node, source, ctx, diagnostics|
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    if !tail_matches_shell_fn(name) {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let mut flagged = false;
    for arg in args.named_children(&mut cursor) {
        if argument_uses_template_interpolation(arg)
            || options_object_has_shell_true(arg, source)
        {
            flagged = true;
            break;
        }
    }
    if flagged {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            "no-shell-exec",
            "Shell interpolation in `exec()` or `shell: true` allows command injection — use `execFile()` with an args array.".into(),
            Severity::Error,
        ));
    }
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_exec_with_template_literal() {
        assert_eq!(run_on("exec(`git ${cmd}`)").len(), 1);
    }

    #[test]
    fn flags_shell_true() {
        assert_eq!(run_on("spawn('sh', ['-c', cmd], { shell: true })").len(), 1);
    }

    #[test]
    fn allows_execfile() {
        assert!(run_on("execFile('git', ['status'])").is_empty());
    }

    #[test]
    fn allows_exec_literal() {
        assert!(run_on("exec('git status')").is_empty());
    }

    #[test]
    fn allows_exec_template_without_substitution() {
        assert!(run_on("exec(`git status`)").is_empty());
    }
}
