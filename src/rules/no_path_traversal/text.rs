use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const FS_OPS: &[&str] = &[
    "fs.readFile(",
    "fs.writeFile(",
    "fs.unlink(",
    "fs.createReadStream(",
    "fs.createWriteStream(",
    "readFile(",
    "writeFile(",
];
const USER_DATA: &[&str] = &[
    "req.params.",
    "req.query.",
    "req.body.",
    "params.",
    "searchParams.get(",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if !FS_OPS.iter().any(|f| t.contains(f)) {
                continue;
            }
            if t.contains("basename(") || t.contains("resolve(") {
                continue;
            }
            if USER_DATA.iter().any(|u| t.contains(u)) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-path-traversal".into(),
                    message: "User-controlled path in `fs` call — use `path.basename()` or validate against a safe root.".into(),
                    severity: Severity::Error,
                    span: None,
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
    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), s))
    }
    #[test]
    fn flags_fs_read_with_req_params() {
        assert_eq!(run("fs.readFile(req.params.filename)").len(), 1);
    }
    #[test]
    fn flags_write_with_query() {
        assert_eq!(run("fs.writeFile(req.query.path, data)").len(), 1);
    }
    #[test]
    fn allows_basename_sanitization() {
        assert!(run("fs.readFile(path.basename(req.params.filename))").is_empty());
    }
    #[test]
    fn allows_literal_path() {
        assert!(run("fs.readFile('/data/file.txt')").is_empty());
    }
}
