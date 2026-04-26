//! node-no-sync backend — flag synchronous Node.js method calls (`*Sync()`).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };

    // Extract the method name from either `fooSync(...)` or `fs.readFileSync(...)`.
    let method_name = match callee.kind() {
        "identifier" => callee.utf8_text(source).unwrap_or(""),
        "member_expression" => {
            let Some(prop) = callee.child_by_field_name("property") else { return };
            prop.utf8_text(source).unwrap_or("")
        }
        _ => return,
    };

    // Must end with "Sync" and have at least one char before it.
    if method_name.len() <= 4 || !method_name.ends_with("Sync") {
        return;
    }

    // Reconstruct the full callee text for the message.
    let full_name = callee.utf8_text(source).unwrap_or(method_name);

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "node-no-sync".into(),
        message: format!("Unexpected sync method: `{full_name}()`. Use the async variant instead."),
        severity: Severity::Warning,
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
    fn flags_read_file_sync() {
        let d = run_on("const data = fs.readFileSync('f.txt');");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("readFileSync"));
    }

    #[test]
    fn flags_exec_sync() {
        assert_eq!(run_on("const out = execSync('ls');").len(), 1);
    }

    #[test]
    fn allows_async_method() {
        assert!(run_on("fs.readFile('f.txt', cb);").is_empty());
    }

    #[test]
    fn allows_non_sync_identifier() {
        assert!(run_on("const isInSync = true;").is_empty());
    }
}
