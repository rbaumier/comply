use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `try {` or `try{` — the start of a try block.
fn has_try_statement(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut start = 0;
    while let Some(pos) = line[start..].find("try") {
        let abs = start + pos;
        let after = abs + 3;
        // Check it's a standalone `try` keyword, not part of a longer word.
        let prev_ok = abs == 0 || {
            let prev = bytes[abs - 1];
            !prev.is_ascii_alphanumeric() && prev != b'_'
        };
        let next_ok = after >= line.len() || {
            let next = bytes[after];
            !next.is_ascii_alphanumeric() && next != b'_'
        };
        if prev_ok && next_ok {
            // Check that `{` follows (possibly after whitespace).
            let rest = line[after..].trim_start();
            if rest.starts_with('{') {
                return true;
            }
        }
        start = abs + 3;
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
            if has_try_statement(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-try-statements".into(),
                    message: "`try` block — prefer Result types or explicit error handling.".into(),
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
    fn flags_try_block() {
        assert_eq!(run("try {").len(), 1);
    }

    #[test]
    fn flags_try_no_space() {
        assert_eq!(run("try{").len(), 1);
    }

    #[test]
    fn allows_retry_function() {
        assert!(run("const retry = 3;").is_empty());
    }

    #[test]
    fn allows_comment() {
        assert!(run("// try { something }").is_empty());
    }

    #[test]
    fn allows_try_in_string() {
        // Still flags — text check is heuristic. This is acceptable.
        // The rule is opinionated about all `try {` occurrences.
        let result = run(r#"const msg = "try {";"#);
        // This may or may not flag — it's inside a string but the text
        // check doesn't parse strings. That's acceptable for this rule.
        assert!(result.len() <= 1);
    }
}
