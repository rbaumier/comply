use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const DEPRECATED_APIS: &[(&str, &str)] = &[
    (
        "new Buffer(",
        "Use `Buffer.from()` or `Buffer.alloc()` instead of `new Buffer()`.",
    ),
    (
        "require('domain')",
        "The `domain` module is deprecated — use structured error handling instead.",
    ),
    (
        "require(\"domain\")",
        "The `domain` module is deprecated — use structured error handling instead.",
    ),
    (
        "fs.exists(",
        "Use `fs.existsSync()`, `fs.stat()`, or `fs.access()` instead of `fs.exists()`.",
    ),
    ("url.parse(", "Use `new URL()` instead of `url.parse()`."),
    (
        "require('punycode')",
        "The `punycode` module is deprecated — use the userland `punycode` package.",
    ),
    (
        "require(\"punycode\")",
        "The `punycode` module is deprecated — use the userland `punycode` package.",
    ),
    (
        "querystring.escape",
        "The `querystring` module is deprecated — use `URLSearchParams` instead.",
    ),
    (
        "util.isArray(",
        "Use `Array.isArray()` instead of `util.isArray()`.",
    ),
    (
        "util.pump(",
        "Use `stream.pipeline()` or `.pipe()` instead of `util.pump()`.",
    ),
    (
        "process.env.NODE_DEBUG",
        "Use the `util.debuglog()` API instead of reading `process.env.NODE_DEBUG` directly.",
    ),
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for &(pattern, message) in DEPRECATED_APIS {
                if line.contains(pattern) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "no-deprecated-api".into(),
                        message: message.into(),
                        severity: Severity::Warning,
                    });
                }
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
    fn flags_new_buffer() {
        assert_eq!(run("const buf = new Buffer(10);").len(), 1);
    }

    #[test]
    fn flags_url_parse() {
        assert_eq!(run("const parsed = url.parse(myUrl);").len(), 1);
    }

    #[test]
    fn flags_require_domain() {
        assert_eq!(run("const d = require('domain');").len(), 1);
    }

    #[test]
    fn allows_buffer_from() {
        assert!(run("const buf = Buffer.from('hello');").is_empty());
    }

    #[test]
    fn allows_new_url() {
        assert!(run("const u = new URL(myUrl);").is_empty());
    }
}
