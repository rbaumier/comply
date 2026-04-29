//! no-unsafe-shell-exec backend — flag calls to shell-exec APIs whose
//! first argument is not a plain string literal (i.e. contains user-
//! controlled or interpolated content).
//!
//! Covers the classic `child_process` APIs (`exec`, `execSync`, `spawn`,
//! `spawnSync`, `execFile`, `execFileSync`) when called via a member
//! expression like `cp.exec(cmd)` or the imported names like `exec(cmd)`.

use crate::diagnostic::{Diagnostic, Severity};

const UNSAFE_FNS: &[&str] = &["exec", "execSync", "spawn", "spawnSync"];
const SAFE_RECEIVERS: &[&str] = &["Regex", "RegExp", "regex", "re", "pattern", "matcher"];

/// Unsafe if the argument isn't a plain string literal. Template strings
/// with substitutions (`${x}`) are unsafe; template strings without
/// substitutions are treated as plain literals.
fn is_unsafe_arg(node: tree_sitter::Node) -> bool {
    match node.kind() {
        "string" => false,
        "template_string" => {
            let mut cursor = node.walk();
            node.children(&mut cursor)
                .any(|c| c.kind() == "template_substitution")
        }
        _ => true,
    }
}

crate::ast_check! { on ["call_expression"] prefilter = ["exec", "spawn"] => |node, source, ctx, diagnostics|
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    // Match last segment after `.`, so both `exec(...)` and `cp.exec(...)` hit.
    let last = name.rsplit('.').next().unwrap_or(name);
    if !UNSAFE_FNS.contains(&last) {
        return;
    }
    if let Some(prefix) = name.rsplit('.').nth(1) {
        let prefix_lower = prefix.to_ascii_lowercase();
        if SAFE_RECEIVERS.iter().any(|r| prefix_lower == *r || prefix_lower.ends_with(r)) {
            return;
        }
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let Some(first) = args.named_child(0) else { return };
    if !is_unsafe_arg(first) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-unsafe-shell-exec".into(),
        message: format!("`{last}()` called with a dynamic command — use `execFile`/`spawn` with an argv array so user input isn't re-parsed by the shell."),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_exec_with_variable() {
        assert_eq!(run_on("exec(cmd);").len(), 1);
    }

    #[test]
    fn flags_exec_with_template_interpolation() {
        assert_eq!(run_on("exec(`ls ${dir}`);").len(), 1);
    }

    #[test]
    fn flags_cp_exec_with_variable() {
        assert_eq!(run_on("cp.exec(cmd);").len(), 1);
    }

    #[test]
    fn flags_exec_sync_with_concat() {
        assert_eq!(run_on(r#"execSync("ls " + dir);"#).len(), 1);
    }

    #[test]
    fn allows_exec_with_string_literal() {
        assert!(run_on(r#"exec("ls");"#).is_empty());
    }

    #[test]
    fn allows_exec_with_template_no_interp() {
        assert!(run_on("exec(`ls`);").is_empty());
    }

    #[test]
    fn allows_unrelated_call() {
        assert!(run_on("runSomething(cmd);").is_empty());
    }

    #[test]
    fn allows_regexp_exec() {
        assert!(run_on("pattern.exec(content);").is_empty());
    }

    #[test]
    fn allows_regex_exec() {
        assert!(run_on("regex.exec(line);").is_empty());
    }
}
