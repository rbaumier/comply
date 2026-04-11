use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const CLEAR_TEXT_PROTOCOLS: &[&str] = &["http://", "ftp://", "telnet://"];
const DEV_PREFIXES: &[&str] = &[
    "http://localhost",
    "http://127.0.0.1",
    "http://0.0.0.0",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if !(line.contains('"') || line.contains('\'') || line.contains('`')) {
                continue;
            }
            for proto in CLEAR_TEXT_PROTOCOLS {
                if !line.contains(proto) {
                    continue;
                }
                // Skip dev-local URLs.
                if DEV_PREFIXES.iter().any(|prefix| line.contains(prefix)) {
                    continue;
                }
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-clear-text-protocol".into(),
                    message: format!("Clear-text protocol `{proto}` detected — use the encrypted equivalent."),
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
    fn flags_http() {
        assert_eq!(run(r#"const url = "http://example.com";"#).len(), 1);
    }

    #[test]
    fn flags_ftp() {
        assert_eq!(run(r#"const url = "ftp://files.example.com";"#).len(), 1);
    }

    #[test]
    fn allows_https() {
        assert!(run(r#"const url = "https://example.com";"#).is_empty());
    }

    #[test]
    fn allows_localhost() {
        assert!(run(r#"const url = "http://localhost:3000";"#).is_empty());
    }

    #[test]
    fn allows_loopback() {
        assert!(run(r#"const url = "http://127.0.0.1:8080";"#).is_empty());
    }
}
