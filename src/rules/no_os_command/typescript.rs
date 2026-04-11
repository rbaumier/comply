use crate::diagnostic::{Diagnostic, Severity};

const DANGEROUS_FUNCTIONS: &[&str] = &["exec", "execSync", "spawn", "spawnSync", "execFile"];

crate::ast_check! { |node, source, ctx, diagnostics|
    match node.kind() {
        "call_expression" => {
            let Some(callee) = node.child_by_field_name("function") else { return };
            let name = match callee.kind() {
                "identifier" => callee.utf8_text(source).unwrap_or(""),
                "member_expression" => {
                    let Some(prop) = callee.child_by_field_name("property") else { return };
                    prop.utf8_text(source).unwrap_or("")
                }
                _ => return,
            };
            if !DANGEROUS_FUNCTIONS.contains(&name) {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-os-command".into(),
                message: format!(
                    "OS command execution via `{name}` — potential command-injection vector.",
                ),
                severity: Severity::Error,
            });
        }
        "import_statement" => {}
        "string" => {
            let text = node.utf8_text(source).unwrap_or("");
            if text.contains("child_process") {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "no-os-command".into(),
                    message: "OS command execution via `child_process` — potential command-injection vector.".into(),
                    severity: Severity::Error,
                });
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_exec() {
        assert_eq!(run("const result = exec('ls -la');").len(), 1);
    }

    #[test]
    fn flags_spawn() {
        assert_eq!(run("const child = spawn('node', ['app.js']);").len(), 1);
    }

    #[test]
    fn flags_child_process_import() {
        assert_eq!(run("import { exec } from 'child_process';").len(), 1);
    }

    #[test]
    fn allows_normal_function_calls() {
        assert!(run("const result = execute(query);").is_empty());
    }
}
