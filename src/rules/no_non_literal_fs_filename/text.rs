use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Common fs methods whose first argument should be a string literal.
const FS_METHODS: &[&str] = &[
    "readFile(",
    "readFileSync(",
    "writeFile(",
    "writeFileSync(",
    "readdir(",
    "readdirSync(",
    "unlink(",
    "unlinkSync(",
    "rmdir(",
    "rmdirSync(",
    "mkdir(",
    "mkdirSync(",
    "stat(",
    "statSync(",
    "lstat(",
    "lstatSync(",
    "rename(",
    "renameSync(",
    "copyFile(",
    "copyFileSync(",
    "access(",
    "accessSync(",
    "appendFile(",
    "appendFileSync(",
    "chmod(",
    "chmodSync(",
    "chown(",
    "chownSync(",
    "createReadStream(",
    "createWriteStream(",
    "open(",
    "openSync(",
    "realpath(",
    "realpathSync(",
    "rm(",
    "rmSync(",
];

/// Check if the argument to an fs method call looks like a string literal.
fn is_literal_arg(after_paren: &str) -> bool {
    let trimmed = after_paren.trim();
    trimmed.starts_with('"')
        || trimmed.starts_with('\'')
        || trimmed.starts_with('`')
        || trimmed.starts_with("__dirname")
        || trimmed.starts_with("__filename")
        || trimmed.starts_with("path.join(")
        || trimmed.starts_with("path.resolve(")
        || trimmed.starts_with("new URL(")
}

fn has_non_literal_fs(line: &str) -> bool {
    // Must contain `fs.` or `fs/promises` import usage.
    if !line.contains("fs.") && !line.contains("fsPromises.") {
        return false;
    }
    for method in FS_METHODS {
        if let Some(pos) = line.find(method) {
            let after = &line[pos + method.len()..];
            if after.trim().starts_with(')') {
                continue; // no args
            }
            if !is_literal_arg(after) {
                return true;
            }
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            if has_non_literal_fs(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-non-literal-fs-filename".into(),
                    message: "Non-literal filename in `fs` call — use a string literal or validate the path.".into(),
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
    fn flags_variable_filename() {
        assert_eq!(run("fs.readFile(userInput, cb);").len(), 1);
    }

    #[test]
    fn flags_variable_in_write() {
        assert_eq!(run("fs.writeFileSync(filePath, data);").len(), 1);
    }

    #[test]
    fn allows_literal_filename() {
        assert!(run(r#"fs.readFile("config.json", cb);"#).is_empty());
    }

    #[test]
    fn allows_path_join() {
        assert!(run("fs.readFile(path.join(__dirname, 'file.txt'), cb);").is_empty());
    }

    #[test]
    fn allows_no_fs_call() {
        assert!(run("const x = 42;").is_empty());
    }
}
