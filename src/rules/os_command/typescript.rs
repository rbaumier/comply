use crate::diagnostic::{Diagnostic, Severity};

const DANGEROUS_FUNCTIONS: &[&str] = &["exec", "execSync", "spawn", "spawnSync"];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }

    let Some(func) = node.child_by_field_name("function") else { return; };

    let func_name = match func.kind() {
        "identifier" => func.utf8_text(source).unwrap_or(""),
        "member_expression" => {
            if let Some(prop) = func.child_by_field_name("property") {
                prop.utf8_text(source).unwrap_or("")
            } else { return; }
        }
        _ => return,
    };

    if !DANGEROUS_FUNCTIONS.contains(&func_name) { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return; };
    let Some(first_arg) = args.named_child(0) else { return; };

    // Flag if first argument is a template literal with expressions or string concatenation
    let is_dynamic = match first_arg.kind() {
        "template_string" => {
            // Check if template has interpolation (${...})
            let mut cursor = first_arg.walk();
            first_arg.children(&mut cursor).any(|c| c.kind() == "template_substitution")
        }
        "binary_expression" => {
            // String concatenation
            if let Some(op) = first_arg.child_by_field_name("operator") {
                op.utf8_text(source).unwrap_or("") == "+"
            } else { false }
        }
        "identifier" | "member_expression" => {
            // Variable — could be user input
            true
        }
        _ => false,
    };

    if !is_dynamic { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "os-command".into(),
        message: format!("`{func_name}()` with dynamic command — potential command injection."),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(code: &str) -> Vec<Diagnostic> { crate::rules::test_helpers::run_ts(code, &Check) }

    #[test]
    fn flags_exec_template() {
        assert_eq!(run("exec(`rm -rf ${userInput}`)").len(), 1);
    }

    #[test]
    fn flags_exec_concat() {
        assert_eq!(run("exec('rm -rf ' + path)").len(), 1);
    }

    #[test]
    fn flags_spawn_variable() {
        assert_eq!(run("spawn(command)").len(), 1);
    }

    #[test]
    fn flags_exec_sync() {
        assert_eq!(run("execSync(`cat ${file}`)").len(), 1);
    }

    #[test]
    fn allows_static_command() {
        assert!(run("exec('ls -la')").is_empty());
    }

    #[test]
    fn allows_exec_file() {
        assert!(run("execFile('rm', ['-rf', path])").is_empty());
    }
}
