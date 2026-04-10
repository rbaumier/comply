use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_bare_timestamp(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "sql-no-timestamp-without-tz".into(),
                    message: "`TIMESTAMP` without timezone — use `TIMESTAMPTZ`. Without TZ, the same instant is interpreted differently depending on the server's timezone setting.".into(),
                    severity: Severity::Error,
                });
            }
        }
        diagnostics
    }
}

fn has_bare_timestamp(line: &str) -> bool {
    let upper = line.to_ascii_uppercase();
    let mut start = 0;
    while let Some(pos) = upper[start..].find("TIMESTAMP") {
        let abs = start + pos;
        let after = &upper[abs + 9..];
        if after.starts_with("TZ") || after.trim_start().starts_with("WITH TIME ZONE") {
            start = abs + 9;
            continue;
        }
        if abs >= 8 && upper[..abs].ends_with("CURRENT_") {
            start = abs + 9;
            continue;
        }
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("t.sql"), source)) }

    #[test]
    fn flags_bare() { assert_eq!(run("created_at TIMESTAMP NOT NULL").len(), 1); }
    #[test]
    fn allows_tz() { assert!(run("created_at TIMESTAMPTZ NOT NULL").is_empty()); }
    #[test]
    fn allows_with_tz() { assert!(run("created_at TIMESTAMP WITH TIME ZONE").is_empty()); }
}
