use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Find identifiers ending in `Sync(` — the calling pattern for synchronous
/// Node.js methods like `readFileSync(`, `writeFileSync(`, `execSync(`, etc.
fn find_sync_call(line: &str) -> Option<&str> {
    let mut start = 0;
    while let Some(pos) = line[start..].find("Sync(") {
        let abs = start + pos;
        // Walk backwards to find the start of the identifier.
        let ident_start = line[..abs]
            .bytes()
            .rposition(|b| !b.is_ascii_alphanumeric() && b != b'_' && b != b'.')
            .map_or(0, |p| p + 1);
        let ident = &line[ident_start..abs + 4]; // include "Sync"
        // Must have at least one char before "Sync".
        if abs > ident_start && !ident.is_empty() {
            return Some(ident);
        }
        start = abs + 5;
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
            if let Some(name) = find_sync_call(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "node-no-sync".into(),
                    message: format!("Unexpected sync method: `{name}()`. Use the async variant instead."),
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
    fn flags_read_file_sync() {
        let d = run("const data = fs.readFileSync('f.txt');");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("readFileSync"));
    }

    #[test]
    fn flags_exec_sync() {
        assert_eq!(run("const out = execSync('ls');").len(), 1);
    }

    #[test]
    fn flags_write_file_sync() {
        assert_eq!(run("fs.writeFileSync('f.txt', data);").len(), 1);
    }

    #[test]
    fn allows_async_method() {
        assert!(run("fs.readFile('f.txt', cb);").is_empty());
    }

    #[test]
    fn allows_comment() {
        assert!(run("// readFileSync('f.txt')").is_empty());
    }

    #[test]
    fn allows_word_sync_not_call() {
        assert!(run("const isInSync = true;").is_empty());
    }
}
