use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// True if the line contains `new require(`.
fn has_new_require(line: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = line[start..].find("new require(") {
        let abs = start + pos;
        // Make sure `new` is not part of a longer identifier.
        if abs > 0 {
            let prev = line.as_bytes()[abs - 1];
            if prev.is_ascii_alphanumeric() || prev == b'_' {
                start = abs + 12;
                continue;
            }
        }
        return true;
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
            if has_new_require(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "node-no-new-require".into(),
                    message: "Unexpected `new require(...)`. Separate the require call: `const Mod = require('...'); new Mod()`.".into(),
                    severity: Severity::Error,
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
    fn flags_new_require() {
        assert_eq!(run("const app = new require('express');").len(), 1);
    }

    #[test]
    fn flags_new_require_start_of_line() {
        assert_eq!(run("new require('foo');").len(), 1);
    }

    #[test]
    fn allows_regular_require() {
        assert!(run("const express = require('express');").is_empty());
    }

    #[test]
    fn allows_new_after_require() {
        assert!(run("const app = new (require('express'))();").is_empty());
    }

    #[test]
    fn allows_comment() {
        assert!(run("// new require('foo')").is_empty());
    }
}
