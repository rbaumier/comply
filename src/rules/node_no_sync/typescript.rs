//! node-no-sync backend — flag synchronous Node.js method calls (`*Sync()`).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["Sync"] => |node, source, ctx, diagnostics|
    if allows_sync_node_api(ctx.path, ctx.source) {
        return;
    }

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

fn allows_sync_node_api(path: &std::path::Path, source: &str) -> bool {
    let lower = path.to_string_lossy().replace('\\', "/").to_ascii_lowercase();
    lower.starts_with("scripts/")
        || lower.contains("/scripts/")
        || lower.starts_with("bin/")
        || lower.contains("/bin/")
        || lower.starts_with("tools/")
        || lower.contains("/tools/")
        || lower.starts_with("cli/")
        || lower.contains("/cli/")
        || file_name_is_config(path)
        || source
            .lines()
            .next()
            .is_some_and(|line| line.starts_with("#!") && line.contains("node"))
}

fn file_name_is_config(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|file_name| file_name.to_str())
        .is_some_and(|file_name| file_name.contains(".config."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    fn run_on_path(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(source, &Check, path)
    }

    #[test]
    fn allows_sync_calls_in_scripts_dir() {
        assert!(run_on_path("const out = execSync('ls');", "scripts/check-dependencies.js").is_empty());
    }

    #[test]
    fn allows_sync_calls_in_config_files() {
        assert!(run_on_path("const data = fs.readFileSync('x');", "svelte.config.js").is_empty());
        assert!(run_on_path("const data = fs.readFileSync('x');", "vite.config.ts").is_empty());
    }

    #[test]
    fn allows_sync_calls_in_node_shebang_cli() {
        let source = "#!/usr/bin/env node\nconst out = execSync('ls');";
        assert!(run_on_path(source, "src/check.js").is_empty());
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
