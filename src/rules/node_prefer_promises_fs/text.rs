use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const FS_METHODS: &[&str] = &[
    "readFile", "writeFile", "appendFile", "copyFile", "mkdir", "mkdtemp",
    "open", "readdir", "readlink", "rename", "rmdir", "rm", "stat", "lstat",
    "unlink", "access", "chmod", "lchmod", "lchown", "chown", "link",
    "symlink", "truncate", "realpath", "utimes",
];

/// Check if the line contains a callback-style `fs.method(` call that is NOT
/// `fs.promises.method(` or a `Sync` variant.
fn find_callback_fs_call(line: &str) -> Option<&str> {
    for method in FS_METHODS {
        let pattern_dot = format!("fs.{method}(");
        let promises_pattern = format!("fs.promises.{method}(");
        let sync_pattern = format!("{method}Sync(");

        let mut start = 0;
        while let Some(pos) = line[start..].find(&pattern_dot) {
            let abs = start + pos;
            // Skip if it's actually `fs.promises.method(`.
            if line[..abs + pattern_dot.len()].contains(&promises_pattern) {
                start = abs + pattern_dot.len();
                continue;
            }
            // Skip Sync variants (handled by node-no-sync).
            if line[start..].contains(&sync_pattern) {
                start = abs + pattern_dot.len();
                continue;
            }
            // Make sure `fs` is not part of a longer identifier.
            if abs > 0 {
                let prev = line.as_bytes()[abs - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' {
                    start = abs + pattern_dot.len();
                    continue;
                }
            }
            return Some(method);
        }
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            if let Some(method) = find_callback_fs_call(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "node-prefer-promises-fs".into(),
                    message: format!("Use `fs.promises.{method}()` instead of callback-based `fs.{method}()`."),
                    severity: Severity::Warning,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_fs_read_file() {
        let d = run("fs.readFile('f.txt', cb);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("fs.promises.readFile"));
    }

    #[test]
    fn flags_fs_write_file() {
        assert_eq!(run("fs.writeFile('f.txt', data, cb);").len(), 1);
    }

    #[test]
    fn flags_fs_mkdir() {
        assert_eq!(run("fs.mkdir('/tmp/dir', cb);").len(), 1);
    }

    #[test]
    fn allows_fs_promises() {
        assert!(run("fs.promises.readFile('f.txt');").is_empty());
    }

    #[test]
    fn allows_comment() {
        assert!(run("// fs.readFile('f.txt', cb)").is_empty());
    }

    #[test]
    fn allows_sync_variant() {
        // Sync variants are handled by node-no-sync, not this rule.
        assert!(run("fs.readFileSync('f.txt');").is_empty());
    }
}
