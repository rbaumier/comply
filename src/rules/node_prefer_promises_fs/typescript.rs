//! node-prefer-promises-fs backend — flag callback-based `fs.*()` calls.

use crate::diagnostic::{Diagnostic, Severity};

const FS_METHODS: &[&str] = &[
    "readFile", "writeFile", "appendFile", "copyFile", "mkdir", "mkdtemp",
    "open", "readdir", "readlink", "rename", "rmdir", "rm", "stat", "lstat",
    "unlink", "access", "chmod", "lchmod", "lchown", "chown", "link",
    "symlink", "truncate", "realpath", "utimes",
];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(obj) = callee.child_by_field_name("object") else { return };
    let Some(prop) = callee.child_by_field_name("property") else { return };
    let method = prop.utf8_text(source).unwrap_or("");

    // Skip Sync variants — handled by node-no-sync.
    if method.ends_with("Sync") {
        return;
    }

    if !FS_METHODS.contains(&method) {
        return;
    }

    // `fs.method(...)` — object is `fs` identifier.
    // `fs.promises.method(...)` — object is `fs.promises` member_expression → skip.
    match obj.kind() {
        "identifier" => {
            if obj.utf8_text(source).unwrap_or("") != "fs" {
                return;
            }
        }
        _ => return,
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "node-prefer-promises-fs".into(),
        message: format!("Use `fs.promises.{method}()` instead of callback-based `fs.{method}()`."),
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
    fn flags_fs_read_file() {
        let d = run_on("fs.readFile('f.txt', cb);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("fs.promises.readFile"));
    }

    #[test]
    fn flags_fs_write_file() {
        assert_eq!(run_on("fs.writeFile('f.txt', data, cb);").len(), 1);
    }

    #[test]
    fn allows_fs_promises() {
        assert!(run_on("fs.promises.readFile('f.txt');").is_empty());
    }

    #[test]
    fn allows_sync_variant() {
        assert!(run_on("fs.readFileSync('f.txt');").is_empty());
    }

    #[test]
    fn allows_other_object() {
        assert!(run_on("myFs.readFile('f.txt', cb);").is_empty());
    }
}
